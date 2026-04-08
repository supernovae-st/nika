// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cursor and Input Helpers for Chat View
//!
//! Contains cursor movement, clipboard operations, and input height calculation.

use tui_input::InputRequest;

use super::hints::detect_verb_in_input;
use super::{ChatMode, ChatView, CurrentVerb, VerbColor};

impl ChatView {
    // ═══════════════════════════════════════════════════════════════════════════
    // Cursor Movement and Input Helpers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Insert character at cursor (delegates to tui-input)
    pub fn insert_char(&mut self, c: char) {
        self.input.handle(InputRequest::InsertChar(c));
        // Track mutation in edit history (coalesces rapid keystrokes)
        self.edit_history
            .push(self.input.value(), self.input.cursor());
        self.update_mode_from_input();
        self.check_mention_trigger();
    }

    /// Delete character before cursor (delegates to tui-input)
    pub fn backspace(&mut self) {
        self.input.handle(InputRequest::DeletePrevChar);
        self.edit_history
            .push(self.input.value(), self.input.cursor());
        self.update_mode_from_input();
        self.check_mention_trigger();
    }

    /// Update chat_mode and current_verb based on input prefix
    /// This syncs the mode indicator with what the user is typing
    pub(super) fn update_mode_from_input(&mut self) {
        let input = self.input.value();

        // Check for verb prefix in input
        if let Some((_, verb_color, is_complete, _)) = detect_verb_in_input(input) {
            // Only update if we have a complete verb (with space after)
            if is_complete {
                // Update chat_mode for Infer/Agent toggle
                match verb_color {
                    VerbColor::Agent => self.chat_mode = ChatMode::Agent,
                    VerbColor::Infer => self.chat_mode = ChatMode::Infer,
                    // Other verbs keep current chat_mode but update current_verb
                    _ => {}
                }

                // Update current_verb for MissionControlPanel
                self.current_verb = match verb_color {
                    VerbColor::Infer => CurrentVerb::Infer,
                    VerbColor::Exec => CurrentVerb::Exec,
                    VerbColor::Fetch => CurrentVerb::Fetch,
                    VerbColor::Invoke => CurrentVerb::Invoke,
                    VerbColor::Agent => CurrentVerb::Agent,
                    VerbColor::Spawn => CurrentVerb::Spawn,
                    VerbColor::User => CurrentVerb::None, // User is not a verb command
                };
            }
        } else if input.is_empty() || !input.starts_with('/') {
            // No verb prefix → reset to defaults
            self.current_verb = CurrentVerb::None;
            // Keep chat_mode as-is when no prefix (user might want to stay in Agent mode)
        }
    }

    /// Move cursor left (delegates to tui-input)
    pub fn cursor_left(&mut self) {
        self.input.handle(InputRequest::GoToPrevChar);
    }

    /// Move cursor right (delegates to tui-input)
    pub fn cursor_right(&mut self) {
        self.input.handle(InputRequest::GoToNextChar);
    }

    /// Move cursor to previous word (Ctrl+Left)
    pub fn cursor_prev_word(&mut self) {
        self.input.handle(InputRequest::GoToPrevWord);
    }

    /// Move cursor to next word (Ctrl+Right)
    pub fn cursor_next_word(&mut self) {
        self.input.handle(InputRequest::GoToNextWord);
    }

    /// Delete previous word (Ctrl+Backspace)
    pub fn delete_prev_word(&mut self) {
        // Force checkpoint before word deletion (significant edit)
        self.edit_history
            .checkpoint(self.input.value(), self.input.cursor());
        self.input.handle(InputRequest::DeletePrevWord);
        self.edit_history
            .push(self.input.value(), self.input.cursor());
        self.update_mode_from_input();
    }

    /// Go to start of input (Home)
    pub fn cursor_start(&mut self) {
        self.input.handle(InputRequest::GoToStart);
    }

    /// Go to end of input (End)
    pub fn cursor_end(&mut self) {
        self.input.handle(InputRequest::GoToEnd);
    }

    /// Copy input to clipboard (Ctrl+C)
    ///
    /// Returns `true` if the copy succeeded, `false` otherwise.
    pub fn copy_to_clipboard(&mut self) -> bool {
        if let Some(clipboard) = &mut self.clipboard {
            match clipboard.set_text(self.input.value().to_string()) {
                Ok(()) => return true,
                Err(e) => {
                    tracing::debug!("Clipboard copy failed: {}", e);
                }
            }
        }
        false
    }

    /// Paste from clipboard (Ctrl+V)
    pub fn paste_from_clipboard(&mut self) {
        if let Some(clipboard) = &mut self.clipboard {
            match clipboard.get_text() {
                Ok(text) => {
                    // Checkpoint before paste (significant edit)
                    self.edit_history
                        .checkpoint(self.input.value(), self.input.cursor());
                    for c in text.chars() {
                        self.input.handle(InputRequest::InsertChar(c));
                    }
                    self.edit_history
                        .push(self.input.value(), self.input.cursor());
                    self.update_mode_from_input();
                }
                Err(e) => {
                    tracing::debug!("Clipboard paste failed: {}", e);
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Dynamic Input Height Helpers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Calculate how many lines the input content requires with word wrapping
    pub(super) fn calculate_input_lines(&self, available_width: u16) -> usize {
        let input_value = self.input.value();
        if input_value.is_empty() {
            return 1;
        }

        // Account for prefix (e.g., "🦋 nika > ") which takes ~12 chars
        let prefix_width = 14_u16;
        let content_width = available_width.saturating_sub(prefix_width + 2); // +2 for borders
        if content_width == 0 {
            return 1;
        }

        let mut total_lines = 0;
        for line in input_value.split('\n') {
            if line.is_empty() {
                total_lines += 1;
            } else {
                // Calculate wrapped lines for this physical line
                let char_count = line.chars().count();
                let lines_needed = (char_count as u16).div_ceil(content_width);
                total_lines += lines_needed.max(1) as usize;
            }
        }
        total_lines.max(1)
    }

    /// Ensure the cursor line is visible by adjusting scroll offset
    pub(super) fn ensure_input_cursor_visible(&mut self, cursor_line: usize, total_lines: usize) {
        if total_lines <= self.input_max_lines {
            self.input_scroll_offset = 0;
            return;
        }

        // Keep a 1-line margin at top/bottom when scrolling
        let margin = 1;

        // If cursor is below visible area, scroll down
        if cursor_line >= self.input_scroll_offset + self.input_max_lines - margin {
            self.input_scroll_offset =
                cursor_line.saturating_sub(self.input_max_lines - 1 - margin);
        }

        // If cursor is above visible area, scroll up
        if cursor_line < self.input_scroll_offset + margin {
            self.input_scroll_offset = cursor_line.saturating_sub(margin);
        }

        // Clamp scroll offset to valid range
        let max_offset = total_lines.saturating_sub(self.input_max_lines);
        self.input_scroll_offset = self.input_scroll_offset.min(max_offset);
    }

    /// Calculate the dynamic input height for layout
    pub(super) fn calculate_input_height(&self, available_width: u16) -> u16 {
        let content_lines = self.calculate_input_lines(available_width);
        let clamped_lines = content_lines.clamp(1, self.input_max_lines);
        (clamped_lines as u16) + 2 // Add 2 for borders
    }

    /// Get the line number where the cursor is (for scroll tracking)
    pub(super) fn get_cursor_line(&self, available_width: u16) -> usize {
        let input_value = self.input.value();
        let cursor_pos = self.input.cursor();
        if input_value.is_empty() {
            return 0;
        }

        let prefix_width = 14_u16;
        let content_width = available_width.saturating_sub(prefix_width + 2);
        if content_width == 0 {
            return 0;
        }

        let mut current_line = 0;
        let mut char_index = 0;

        for line in input_value.split('\n') {
            let line_len = line.chars().count();

            if char_index + line_len >= cursor_pos {
                // Cursor is in this line - calculate wrapped position
                let pos_in_line = cursor_pos - char_index;
                let wrapped_line_offset = pos_in_line / content_width as usize;
                return current_line + wrapped_line_offset;
            }

            // Add wrapped lines for this physical line
            let lines_for_this = if line.is_empty() {
                1
            } else {
                ((line_len as u16).div_ceil(content_width)).max(1) as usize
            };

            current_line += lines_for_this;
            char_index += line_len + 1; // +1 for newline
        }

        current_line
    }
}
