// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Text Selection for Chat View
//!
//! Contains text selection, copy, and clipboard operations.

use super::{ChatPanel, ChatView, MessageRole};

/// Convert character offset to byte offset in a UTF-8 string
/// This handles multi-byte characters correctly
fn char_to_byte_offset(s: &str, char_offset: usize) -> usize {
    s.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

impl ChatView {
    /// Get the selected text (if any)
    pub fn get_selected_text(&self) -> Option<String> {
        let selection = self.text_selection.as_ref()?;
        let (start, end) = selection.normalized();

        let mut result = String::new();

        for (idx, msg) in self.messages.iter().enumerate() {
            if idx < start.message_index || idx > end.message_index {
                continue;
            }

            let content = &msg.content;

            if idx == start.message_index && idx == end.message_index {
                // Single message selection
                let start_byte = char_to_byte_offset(content, start.char_offset);
                let end_byte = char_to_byte_offset(content, end.char_offset);
                if start_byte < content.len() && end_byte <= content.len() {
                    result.push_str(&content[start_byte..end_byte]);
                }
            } else if idx == start.message_index {
                // First message of multi-message selection
                let start_byte = char_to_byte_offset(content, start.char_offset);
                if start_byte < content.len() {
                    result.push_str(&content[start_byte..]);
                    result.push('\n');
                }
            } else if idx == end.message_index {
                // Last message of multi-message selection
                let end_byte = char_to_byte_offset(content, end.char_offset);
                if end_byte <= content.len() {
                    result.push_str(&content[..end_byte]);
                }
            } else {
                // Middle messages - fully selected
                result.push_str(content);
                result.push('\n');
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Copy selection to clipboard
    /// Returns true if copy succeeded
    pub fn copy_selection(&mut self) -> bool {
        if let Some(text) = self.get_selected_text() {
            if let Some(ref mut clipboard) = self.clipboard {
                if clipboard.set_text(&text).is_ok() {
                    // Flash effect for feedback
                    if let Some(ref selection) = self.text_selection {
                        let (start, _) = selection.normalized();
                        self.copy_flash_index = Some(start.message_index);
                        self.copy_flash_start = self.frame;
                        self.cached_msg_dirty = true;
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Clear the current text selection
    pub fn clear_selection(&mut self) {
        self.text_selection = None;
        self.is_selecting = false;
    }

    /// Copy the currently selected message to clipboard
    /// Returns true if copy succeeded
    pub fn copy_selected_message(&mut self, text_only: bool) -> bool {
        // Only works when conversation panel is focused
        if self.focused_panel != ChatPanel::Conversation {
            return false;
        }

        let cursor = self.conversation_scroll.cursor;
        if cursor >= self.messages.len() {
            return false;
        }

        let msg = &self.messages[cursor];
        let text = if text_only {
            // Just the content
            msg.content.clone()
        } else {
            // Full message with role and timestamp
            let role = match msg.role {
                MessageRole::User => "User",
                MessageRole::Nika => "Nika",
                MessageRole::System => "System",
                MessageRole::Tool => "Tool",
            };
            format!("[{}] {}", role, msg.content)
        };

        // Copy to clipboard
        if let Some(ref mut clipboard) = self.clipboard {
            let success = clipboard.set_text(text).is_ok();
            if success {
                // WOW: Trigger flash effect on copied message
                self.copy_flash_index = Some(cursor);
                self.copy_flash_start = self.frame;
                self.cached_msg_dirty = true;
            }
            success
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_char_to_byte_offset_ascii() {
        let s = "hello world";
        assert_eq!(char_to_byte_offset(s, 0), 0);
        assert_eq!(char_to_byte_offset(s, 5), 5);
        assert_eq!(char_to_byte_offset(s, 11), 11);
        assert_eq!(char_to_byte_offset(s, 100), 11); // Past end
    }

    #[test]
    fn test_char_to_byte_offset_unicode() {
        let s = "héllo 🦋 world"; // é = 2 bytes, 🦋 = 4 bytes
        assert_eq!(char_to_byte_offset(s, 0), 0); // 'h'
        assert_eq!(char_to_byte_offset(s, 1), 1); // 'é' starts at byte 1
        assert_eq!(char_to_byte_offset(s, 2), 3); // 'l' starts at byte 3 (after 'é')
        assert_eq!(char_to_byte_offset(s, 6), 7); // '🦋' starts at byte 7
        assert_eq!(char_to_byte_offset(s, 7), 11); // ' ' after butterfly
    }
}
