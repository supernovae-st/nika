//! Input Mode for Vim-style navigation
//!
//! Defines the modal input system used throughout the TUI.

/// Input mode for vim-style navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    /// Normal mode - navigation and commands
    #[default]
    Normal,
    /// Insert mode - text input (chat, search)
    Insert,
    /// Command mode - : prefix commands
    Command,
    /// Search mode - / or ? prefix
    Search,
}

impl InputMode {
    /// Returns the mode indicator character for status bar
    pub fn indicator(&self) -> &'static str {
        match self {
            InputMode::Normal => "N",
            InputMode::Insert => "I",
            InputMode::Command => ":",
            InputMode::Search => "/",
        }
    }

    /// Returns true if text input should be captured
    pub fn captures_input(&self) -> bool {
        matches!(
            self,
            InputMode::Insert | InputMode::Command | InputMode::Search
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_normal() {
        assert_eq!(InputMode::default(), InputMode::Normal);
    }

    #[test]
    fn test_indicator() {
        assert_eq!(InputMode::Normal.indicator(), "N");
        assert_eq!(InputMode::Insert.indicator(), "I");
        assert_eq!(InputMode::Command.indicator(), ":");
        assert_eq!(InputMode::Search.indicator(), "/");
    }

    #[test]
    fn test_captures_input() {
        assert!(!InputMode::Normal.captures_input());
        assert!(InputMode::Insert.captures_input());
        assert!(InputMode::Command.captures_input());
        assert!(InputMode::Search.captures_input());
    }
}
