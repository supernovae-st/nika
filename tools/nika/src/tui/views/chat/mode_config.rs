//! Mode and Configuration for Chat View
//!
//! Contains mode switching, configuration, and utility methods.

use super::{ChatMode, ChatView, MessageRole};
use crate::tui::state::{ChatOverlayMessage, ChatOverlayMessageRole, ChatOverlayState};

// ═══════════════════════════════════════════════════════════════════════════════
// Mode Switching Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Toggle between Infer and Agent modes
    pub fn toggle_mode(&mut self) {
        self.chat_mode = match self.chat_mode {
            ChatMode::Infer => ChatMode::Agent,
            ChatMode::Agent => ChatMode::Infer,
        };
    }

    /// Toggle deep thinking (extended_thinking)
    pub fn toggle_deep_thinking(&mut self) {
        self.deep_thinking = !self.deep_thinking;
    }

    /// Set chat mode directly
    pub fn set_chat_mode(&mut self, mode: ChatMode) {
        self.chat_mode = mode;
    }

    /// Set provider name for display
    pub fn set_provider(&mut self, name: impl Into<String>) {
        self.provider_name = name.into();
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// State Access Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Convert ChatView state to ChatOverlayState for session persistence
    ///
    /// Maps ChatMessage (with full metadata) to ChatOverlayMessage (role + content only)
    /// for saving to .nika/sessions/
    pub fn get_chat_state(&self) -> ChatOverlayState {
        // Convert messages to overlay format (role + content only)
        let messages = self
            .messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    MessageRole::User => ChatOverlayMessageRole::User,
                    MessageRole::Nika => ChatOverlayMessageRole::Nika,
                    MessageRole::System => ChatOverlayMessageRole::System,
                    MessageRole::Tool => ChatOverlayMessageRole::Tool,
                };
                ChatOverlayMessage::new(role, &msg.content)
            })
            .collect();

        ChatOverlayState {
            messages,
            input: self.input.value().to_string(),
            cursor: self.input.cursor(),
            scroll: self.scroll,
            history: self.history.clone(),
            history_index: self.history_index,
            is_streaming: self.is_streaming,
            partial_response: self.partial_response.clone(),
            current_model: self.current_model.clone(),
            edit_history: Default::default(), // Session doesn't persist edit history
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Animation and Update Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Clear flash effect after duration (called each frame in tick)
    pub fn tick_flash(&mut self) {
        // Flash lasts about 16 frames (~250ms at 60fps)
        if self.copy_flash_index.is_some() {
            let elapsed = self.frame.wrapping_sub(self.copy_flash_start);
            if elapsed > 16 {
                self.copy_flash_index = None;
            }
        }
    }

    /// Update scroll state totals from current data
    pub fn update_scroll_totals(&mut self) {
        self.conversation_scroll.set_total(self.messages.len());
        self.activity_scroll.set_total(self.activity_items.len());
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helper Functions
// ═══════════════════════════════════════════════════════════════════════════════

/// Convert character offset to byte offset in a UTF-8 string
/// This handles multi-byte characters correctly
pub(super) fn char_to_byte_offset(s: &str, char_offset: usize) -> usize {
    s.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

// centered_rect moved to widgets/utils.rs (shared utility)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_char_to_byte_offset_ascii() {
        let s = "hello world";
        assert_eq!(char_to_byte_offset(s, 0), 0);
        assert_eq!(char_to_byte_offset(s, 5), 5);
        assert_eq!(char_to_byte_offset(s, 11), 11);
        assert_eq!(char_to_byte_offset(s, 100), 11); // Beyond end returns len
    }

    #[test]
    fn test_char_to_byte_offset_unicode() {
        let s = "héllo 🦋 wörld";
        // h=1, é=2, l=1, l=1, o=1, ' '=1, 🦋=4, ' '=1, w=1, ö=2, r=1, l=1, d=1
        assert_eq!(char_to_byte_offset(s, 0), 0); // 'h'
        assert_eq!(char_to_byte_offset(s, 1), 1); // 'é' starts at byte 1
        assert_eq!(char_to_byte_offset(s, 2), 3); // 'l' starts at byte 3 (after 2-byte é)
        assert_eq!(char_to_byte_offset(s, 6), 7); // '🦋' starts at byte 7
        assert_eq!(char_to_byte_offset(s, 7), 11); // ' ' after butterfly starts at byte 11
    }

    #[test]
    fn test_centered_rect() {
        use crate::tui::widgets::centered_rect;
        use ratatui::layout::Rect;
        let area = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(60, 80, area);

        // Should be roughly centered
        assert!(centered.x > 0);
        assert!(centered.y > 0);
        assert!(centered.width < area.width);
        assert!(centered.height < area.height);
    }

    #[test]
    fn test_toggle_mode() {
        let mut view = ChatView::new();
        assert!(matches!(view.chat_mode, ChatMode::Infer));

        view.toggle_mode();
        assert!(matches!(view.chat_mode, ChatMode::Agent));

        view.toggle_mode();
        assert!(matches!(view.chat_mode, ChatMode::Infer));
    }

    #[test]
    fn test_toggle_deep_thinking() {
        let mut view = ChatView::new();
        assert!(!view.deep_thinking);

        view.toggle_deep_thinking();
        assert!(view.deep_thinking);

        view.toggle_deep_thinking();
        assert!(!view.deep_thinking);
    }

    #[test]
    fn test_set_chat_mode() {
        let mut view = ChatView::new();
        view.set_chat_mode(ChatMode::Agent);
        assert!(matches!(view.chat_mode, ChatMode::Agent));

        view.set_chat_mode(ChatMode::Infer);
        assert!(matches!(view.chat_mode, ChatMode::Infer));
    }

    #[test]
    fn test_set_provider() {
        let mut view = ChatView::new();
        view.set_provider("OpenAI GPT-4");
        assert_eq!(view.provider_name, "OpenAI GPT-4");
    }
}
