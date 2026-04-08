// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Conversation Search for Chat View
//!
//! Contains Ctrl+F search mode, result navigation, and query handling.

use super::ChatView;

// ═══════════════════════════════════════════════════════════════════════════════
// Conversation Search (Ctrl+F)
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Enter search mode (Ctrl+F)
    pub fn start_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.search_results.clear();
        self.search_current = 0;
    }

    /// Exit search mode (Esc)
    pub fn exit_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_results.clear();
        self.search_current = 0;
    }

    /// Update search results when query changes
    pub fn update_search(&mut self) {
        self.search_results.clear();
        self.search_current = 0;

        if self.search_query.is_empty() {
            return;
        }

        let query_lower = self.search_query.to_lowercase();

        // Search through all messages
        // Avoid O(n²) Vec::contains() by checking content OR thinking
        for (idx, msg) in self.messages.iter().enumerate() {
            let content_matches = msg.content.to_lowercase().contains(&query_lower);
            let thinking_matches = msg
                .thinking
                .as_ref()
                .is_some_and(|t| t.to_lowercase().contains(&query_lower));

            // Add index if either content or thinking matches (no duplicates possible)
            if content_matches || thinking_matches {
                self.search_results.push(idx);
            }
        }

        // Scroll to first result if any
        if !self.search_results.is_empty() {
            self.scroll_to_search_result();
        }
    }

    /// Navigate to next search result (Enter or Down)
    pub fn next_search_result(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        self.search_current = (self.search_current + 1) % self.search_results.len();
        self.scroll_to_search_result();
    }

    /// Navigate to previous search result (Up)
    pub fn prev_search_result(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        if self.search_current == 0 {
            self.search_current = self.search_results.len() - 1;
        } else {
            self.search_current -= 1;
        }
        self.scroll_to_search_result();
    }

    /// Scroll to the current search result
    /// Uses ensure_cursor_visible() for consistent SCROLL_MARGIN behavior
    fn scroll_to_search_result(&mut self) {
        if let Some(&msg_idx) = self.search_results.get(self.search_current) {
            // Set cursor to the matching message and ensure it's visible
            self.conversation_scroll.cursor = msg_idx;
            self.conversation_scroll.ensure_cursor_visible();
            self.user_at_bottom = false;
        }
    }

    /// Handle character input in search mode
    pub fn search_input_char(&mut self, c: char) {
        self.search_query.push(c);
        self.update_search();
    }

    /// Handle backspace in search mode
    pub fn search_input_backspace(&mut self) {
        self.search_query.pop();
        self.update_search();
    }
}
