// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Keyboard Actions for TaskBox Widgets
//!
//! Unified shortcut system across all 5 verb boxes.

use crossterm::event::KeyCode;

/// Keyboard action for TaskBox interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    // === Common Actions ===
    /// Copy content to clipboard
    Copy,
    /// Export as JSON
    Json,
    /// Toggle expand/collapse
    Toggle,
    /// Close/deselect
    Close,
    /// Cycle render mode
    Mode,

    // === Exec-specific ===
    /// Kill running process
    Kill,
    /// Re-run command
    Rerun,

    // === Fetch-specific ===
    /// Retry request
    Retry,

    // === Agent-specific ===
    /// Expand/collapse children
    Children,
    /// Show thinking
    Thinking,
}

impl KeyAction {
    /// Parse key code into action
    pub fn from_key(key: KeyCode) -> Option<Self> {
        match key {
            // Common
            KeyCode::Char('c') => Some(Self::Copy),
            KeyCode::Char('j') => Some(Self::Json),
            KeyCode::Enter => Some(Self::Toggle),
            KeyCode::Esc => Some(Self::Close),
            KeyCode::Char('m') => Some(Self::Mode),

            // Exec
            KeyCode::Char('k') => Some(Self::Kill),
            KeyCode::Char('!') => Some(Self::Rerun),

            // Fetch
            KeyCode::Char('r') => Some(Self::Retry),

            // Agent
            KeyCode::Char('e') => Some(Self::Children),
            KeyCode::Char('t') => Some(Self::Thinking),

            _ => None,
        }
    }

    /// Human-readable label
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Json => "JSON",
            Self::Toggle => "Toggle",
            Self::Close => "Close",
            Self::Mode => "Mode",
            Self::Kill => "Kill",
            Self::Rerun => "Re-run",
            Self::Retry => "Retry",
            Self::Children => "Children",
            Self::Thinking => "Thinking",
        }
    }

    /// Keyboard hint for display
    pub const fn hint(&self) -> &'static str {
        match self {
            Self::Copy => "c",
            Self::Json => "j",
            Self::Toggle => "↵",
            Self::Close => "esc",
            Self::Mode => "m",
            Self::Kill => "k",
            Self::Rerun => "!",
            Self::Retry => "r",
            Self::Children => "e",
            Self::Thinking => "t",
        }
    }

    /// Get available actions for a verb type
    pub fn actions_for_verb(verb: &str) -> &'static [Self] {
        match verb {
            "infer" => &[Self::Copy, Self::Json, Self::Toggle, Self::Mode],
            "exec" => &[
                Self::Copy,
                Self::Kill,
                Self::Rerun,
                Self::Toggle,
                Self::Mode,
            ],
            "fetch" => &[
                Self::Copy,
                Self::Json,
                Self::Retry,
                Self::Toggle,
                Self::Mode,
            ],
            "invoke" => &[Self::Copy, Self::Json, Self::Toggle, Self::Mode],
            "agent" => &[
                Self::Copy,
                Self::Json,
                Self::Children,
                Self::Thinking,
                Self::Mode,
            ],
            _ => &[Self::Copy, Self::Toggle, Self::Mode],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_actions() {
        assert_eq!(
            KeyAction::from_key(KeyCode::Char('c')),
            Some(KeyAction::Copy)
        );
        assert_eq!(
            KeyAction::from_key(KeyCode::Char('j')),
            Some(KeyAction::Json)
        );
        assert_eq!(KeyAction::from_key(KeyCode::Enter), Some(KeyAction::Toggle));
        assert_eq!(KeyAction::from_key(KeyCode::Esc), Some(KeyAction::Close));
    }

    #[test]
    fn test_verb_specific_actions() {
        // Exec-specific
        assert_eq!(
            KeyAction::from_key(KeyCode::Char('k')),
            Some(KeyAction::Kill)
        );

        // Fetch-specific
        assert_eq!(
            KeyAction::from_key(KeyCode::Char('r')),
            Some(KeyAction::Retry)
        );
    }

    #[test]
    fn test_unknown_key() {
        assert_eq!(KeyAction::from_key(KeyCode::Char('z')), None);
        assert_eq!(KeyAction::from_key(KeyCode::F(12)), None);
    }

    #[test]
    fn test_action_label() {
        assert_eq!(KeyAction::Copy.label(), "Copy");
        assert_eq!(KeyAction::Json.label(), "JSON");
        assert_eq!(KeyAction::Kill.label(), "Kill");
    }

    #[test]
    fn test_action_hint() {
        assert_eq!(KeyAction::Copy.hint(), "c");
        assert_eq!(KeyAction::Json.hint(), "j");
        assert_eq!(KeyAction::Toggle.hint(), "↵");
    }

    #[test]
    fn test_actions_for_verb_infer() {
        let actions = KeyAction::actions_for_verb("infer");
        assert!(actions.contains(&KeyAction::Copy));
        assert!(actions.contains(&KeyAction::Json));
        assert!(actions.contains(&KeyAction::Toggle));
        assert!(actions.contains(&KeyAction::Mode));
        assert!(!actions.contains(&KeyAction::Kill)); // Not infer-specific
    }

    #[test]
    fn test_actions_for_verb_exec() {
        let actions = KeyAction::actions_for_verb("exec");
        assert!(actions.contains(&KeyAction::Copy));
        assert!(actions.contains(&KeyAction::Kill));
        assert!(actions.contains(&KeyAction::Rerun));
        assert!(actions.contains(&KeyAction::Toggle));
        assert!(!actions.contains(&KeyAction::Json)); // Not exec-specific
    }

    #[test]
    fn test_actions_for_verb_fetch() {
        let actions = KeyAction::actions_for_verb("fetch");
        assert!(actions.contains(&KeyAction::Retry));
        assert!(actions.contains(&KeyAction::Json));
        assert!(!actions.contains(&KeyAction::Kill)); // Not fetch-specific
    }

    #[test]
    fn test_actions_for_verb_agent() {
        let actions = KeyAction::actions_for_verb("agent");
        assert!(actions.contains(&KeyAction::Children));
        assert!(actions.contains(&KeyAction::Thinking));
        assert!(!actions.contains(&KeyAction::Kill)); // Not agent-specific
    }

    #[test]
    fn test_actions_for_unknown_verb() {
        let actions = KeyAction::actions_for_verb("unknown");
        assert!(actions.contains(&KeyAction::Copy));
        assert!(actions.contains(&KeyAction::Toggle));
        assert!(actions.contains(&KeyAction::Mode));
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn test_mode_key() {
        assert_eq!(
            KeyAction::from_key(KeyCode::Char('m')),
            Some(KeyAction::Mode)
        );
    }

    #[test]
    fn test_close_key() {
        assert_eq!(KeyAction::from_key(KeyCode::Esc), Some(KeyAction::Close));
        assert_eq!(KeyAction::Close.label(), "Close");
        assert_eq!(KeyAction::Close.hint(), "esc");
    }

    #[test]
    fn test_rerun_key() {
        assert_eq!(
            KeyAction::from_key(KeyCode::Char('!')),
            Some(KeyAction::Rerun)
        );
        assert_eq!(KeyAction::Rerun.label(), "Re-run");
        assert_eq!(KeyAction::Rerun.hint(), "!");
    }

    #[test]
    fn test_children_key() {
        assert_eq!(
            KeyAction::from_key(KeyCode::Char('e')),
            Some(KeyAction::Children)
        );
        assert_eq!(KeyAction::Children.label(), "Children");
        assert_eq!(KeyAction::Children.hint(), "e");
    }

    #[test]
    fn test_thinking_key() {
        assert_eq!(
            KeyAction::from_key(KeyCode::Char('t')),
            Some(KeyAction::Thinking)
        );
        assert_eq!(KeyAction::Thinking.label(), "Thinking");
        assert_eq!(KeyAction::Thinking.hint(), "t");
    }
}
