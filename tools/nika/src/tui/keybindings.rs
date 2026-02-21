//! Unified Keybindings for Navigation 2.0
//!
//! Centralizes keybinding definitions for consistent behavior across views.

use crossterm::event::{KeyCode, KeyModifiers};

use super::mode::InputMode;
use super::views::TuiView;

/// Keybinding definition
#[derive(Debug, Clone)]
pub struct Keybinding {
    /// Key code
    pub code: KeyCode,
    /// Required modifiers (Ctrl, Shift, Alt)
    pub modifiers: KeyModifiers,
    /// Description for help display
    pub description: &'static str,
    /// Category for grouping in help
    pub category: KeyCategory,
}

/// Keybinding categories for help organization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCategory {
    /// Global bindings (work everywhere)
    Global,
    /// Navigation between views
    ViewNav,
    /// Panel focus navigation
    PanelNav,
    /// Input mode switching
    Mode,
    /// Scrolling
    Scroll,
    /// Actions (copy, export, etc.)
    Action,
    /// Chat-specific
    Chat,
    /// Monitor-specific
    Monitor,
}

impl KeyCategory {
    pub fn label(&self) -> &'static str {
        match self {
            KeyCategory::Global => "Global",
            KeyCategory::ViewNav => "View Navigation",
            KeyCategory::PanelNav => "Panel Focus",
            KeyCategory::Mode => "Mode",
            KeyCategory::Scroll => "Scroll",
            KeyCategory::Action => "Actions",
            KeyCategory::Chat => "Chat",
            KeyCategory::Monitor => "Monitor",
        }
    }
}

/// Get keybindings for current context
pub fn keybindings_for_context(view: TuiView, mode: InputMode) -> Vec<Keybinding> {
    let mut bindings = Vec::new();

    // Global bindings (always available)
    bindings.push(Keybinding {
        code: KeyCode::Char('q'),
        modifiers: KeyModifiers::NONE,
        description: "Quit",
        category: KeyCategory::Global,
    });
    bindings.push(Keybinding {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        description: "Force quit",
        category: KeyCategory::Global,
    });
    bindings.push(Keybinding {
        code: KeyCode::Char('?'),
        modifiers: KeyModifiers::NONE,
        description: "Help",
        category: KeyCategory::Global,
    });

    // View navigation (in Normal mode)
    if mode == InputMode::Normal {
        bindings.push(Keybinding {
            code: KeyCode::Char('1'),
            modifiers: KeyModifiers::NONE,
            description: "Chat view",
            category: KeyCategory::ViewNav,
        });
        bindings.push(Keybinding {
            code: KeyCode::Char('2'),
            modifiers: KeyModifiers::NONE,
            description: "Home view",
            category: KeyCategory::ViewNav,
        });
        bindings.push(Keybinding {
            code: KeyCode::Char('3'),
            modifiers: KeyModifiers::NONE,
            description: "Studio view",
            category: KeyCategory::ViewNav,
        });
        bindings.push(Keybinding {
            code: KeyCode::Char('4'),
            modifiers: KeyModifiers::NONE,
            description: "Monitor view",
            category: KeyCategory::ViewNav,
        });
    }

    // Panel navigation
    bindings.push(Keybinding {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        description: "Next panel",
        category: KeyCategory::PanelNav,
    });
    bindings.push(Keybinding {
        code: KeyCode::BackTab,
        modifiers: KeyModifiers::SHIFT,
        description: "Previous panel",
        category: KeyCategory::PanelNav,
    });

    // Mode switching
    if view == TuiView::Chat && mode == InputMode::Normal {
        bindings.push(Keybinding {
            code: KeyCode::Char('i'),
            modifiers: KeyModifiers::NONE,
            description: "Enter Insert mode",
            category: KeyCategory::Mode,
        });
    }
    if mode == InputMode::Insert {
        bindings.push(Keybinding {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            description: "Return to Normal mode",
            category: KeyCategory::Mode,
        });
    }

    // Scrolling (vim-style)
    if mode == InputMode::Normal {
        bindings.push(Keybinding {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
            description: "Scroll down",
            category: KeyCategory::Scroll,
        });
        bindings.push(Keybinding {
            code: KeyCode::Char('k'),
            modifiers: KeyModifiers::NONE,
            description: "Scroll up",
            category: KeyCategory::Scroll,
        });
        bindings.push(Keybinding {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::NONE,
            description: "Go to top",
            category: KeyCategory::Scroll,
        });
        bindings.push(Keybinding {
            code: KeyCode::Char('G'),
            modifiers: KeyModifiers::SHIFT,
            description: "Go to bottom",
            category: KeyCategory::Scroll,
        });
    }

    // View-specific bindings
    match view {
        TuiView::Chat => {
            if mode == InputMode::Insert {
                bindings.push(Keybinding {
                    code: KeyCode::Enter,
                    modifiers: KeyModifiers::NONE,
                    description: "Send message",
                    category: KeyCategory::Chat,
                });
            }
        }
        TuiView::Monitor => {
            if mode == InputMode::Normal {
                bindings.push(Keybinding {
                    code: KeyCode::Char(' '),
                    modifiers: KeyModifiers::NONE,
                    description: "Toggle pause",
                    category: KeyCategory::Monitor,
                });
                bindings.push(Keybinding {
                    code: KeyCode::Char('r'),
                    modifiers: KeyModifiers::NONE,
                    description: "Retry workflow",
                    category: KeyCategory::Monitor,
                });
                bindings.push(Keybinding {
                    code: KeyCode::Char('y'),
                    modifiers: KeyModifiers::NONE,
                    description: "Yank (copy to clipboard)",
                    category: KeyCategory::Action,
                });
                bindings.push(Keybinding {
                    code: KeyCode::Char('e'),
                    modifiers: KeyModifiers::NONE,
                    description: "Export trace",
                    category: KeyCategory::Action,
                });
            }
        }
        TuiView::Home => {
            if mode == InputMode::Normal {
                bindings.push(Keybinding {
                    code: KeyCode::Enter,
                    modifiers: KeyModifiers::NONE,
                    description: "Run workflow",
                    category: KeyCategory::Action,
                });
            }
        }
        TuiView::Studio => {
            if mode == InputMode::Normal {
                bindings.push(Keybinding {
                    code: KeyCode::Char('s'),
                    modifiers: KeyModifiers::CONTROL,
                    description: "Save file",
                    category: KeyCategory::Action,
                });
            }
        }
    }

    bindings
}

/// Format keybinding for display
pub fn format_key(code: KeyCode, modifiers: KeyModifiers) -> String {
    let mut parts = Vec::new();

    if modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt");
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift");
    }

    let key = match code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "Tab".to_string(), // Shift is already added
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Del".to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        KeyCode::Left => "←".to_string(),
        KeyCode::Right => "→".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PgUp".to_string(),
        KeyCode::PageDown => "PgDn".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        _ => "?".to_string(),
    };

    parts.push(&key);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keybindings_for_chat_normal() {
        let bindings = keybindings_for_context(TuiView::Chat, InputMode::Normal);
        assert!(bindings.iter().any(|b| b.code == KeyCode::Char('i')));
        assert!(bindings.iter().any(|b| b.code == KeyCode::Char('q')));
    }

    #[test]
    fn test_keybindings_for_chat_insert() {
        let bindings = keybindings_for_context(TuiView::Chat, InputMode::Insert);
        assert!(bindings.iter().any(|b| b.code == KeyCode::Esc));
        assert!(bindings.iter().any(|b| b.code == KeyCode::Enter));
    }

    #[test]
    fn test_format_key_simple() {
        assert_eq!(format_key(KeyCode::Char('q'), KeyModifiers::NONE), "q");
        assert_eq!(format_key(KeyCode::Enter, KeyModifiers::NONE), "Enter");
    }

    #[test]
    fn test_format_key_with_modifiers() {
        assert_eq!(
            format_key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            "Ctrl+c"
        );
        assert_eq!(format_key(KeyCode::Tab, KeyModifiers::SHIFT), "Shift+Tab");
    }

    #[test]
    fn test_key_category_labels() {
        assert_eq!(KeyCategory::Global.label(), "Global");
        assert_eq!(KeyCategory::Mode.label(), "Mode");
        assert_eq!(KeyCategory::ViewNav.label(), "View Navigation");
    }
}
