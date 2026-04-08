// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Which-Key Popup Widget
//!
//! Shows available keybindings when a prefix key is pressed (vim-style leader popup).
//!
//! # Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────────────────────┐
//! │  Which Key pressed: g                                         │
//! ├───────────────────────────────────────────────────────────────┤
//! │  g   Go to top           │  e   End of file                  │
//! │  j   Join lines          │  d   Go to definition             │
//! │  c   Comment toggle      │  r   References                   │
//! └───────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Clear, Widget},
};
use std::time::{Duration, Instant};

use unicode_width::UnicodeWidthStr;

use crate::tokens::compat;
use crate::unicode::truncate_to_width;

/// Timeout before which-key popup appears (ms)
const POPUP_DELAY_MS: u64 = 300;

/// Timeout after which popup closes automatically (ms)
const POPUP_TIMEOUT_MS: u64 = 3000;

/// A keybinding entry in the which-key popup
#[derive(Debug, Clone)]
pub struct WhichKeyBinding {
    /// The key to press (after the prefix)
    pub key: char,
    /// Description of what this key does
    pub description: &'static str,
    /// Icon (optional)
    pub icon: Option<&'static str>,
}

impl WhichKeyBinding {
    pub fn new(key: char, description: &'static str) -> Self {
        Self {
            key,
            description,
            icon: None,
        }
    }

    pub fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// A group of keybindings for a prefix
#[derive(Debug, Clone)]
pub struct WhichKeyGroup {
    /// The prefix key that triggers this group
    pub prefix: char,
    /// Display name for the prefix
    pub name: &'static str,
    /// Available bindings after this prefix
    pub bindings: Vec<WhichKeyBinding>,
}

impl WhichKeyGroup {
    pub fn new(prefix: char, name: &'static str) -> Self {
        Self {
            prefix,
            name,
            bindings: Vec::new(),
        }
    }

    pub fn with_bindings(mut self, bindings: Vec<WhichKeyBinding>) -> Self {
        self.bindings = bindings;
        self
    }

    pub fn add(mut self, key: char, description: &'static str) -> Self {
        self.bindings.push(WhichKeyBinding::new(key, description));
        self
    }

    pub fn add_with_icon(
        mut self,
        key: char,
        description: &'static str,
        icon: &'static str,
    ) -> Self {
        self.bindings
            .push(WhichKeyBinding::new(key, description).with_icon(icon));
        self
    }
}

/// Default which-key groups for Nika TUI
pub fn default_which_key_groups() -> Vec<WhichKeyGroup> {
    vec![
        // g prefix - Go/Goto commands
        WhichKeyGroup::new('g', "Go to")
            .add_with_icon('g', "Top of file", "↑")
            .add_with_icon('e', "End of file", "↓")
            .add_with_icon('d', "Definition", "→")
            .add_with_icon('r', "References", "◎")
            .add_with_icon('i', "Implementation", "⚙")
            .add_with_icon('t', "Type definition", "T"),
        // z prefix - View/Fold commands
        WhichKeyGroup::new('z', "View/Fold")
            .add_with_icon('z', "Center cursor", "◉")
            .add_with_icon('t', "Cursor to top", "↑")
            .add_with_icon('b', "Cursor to bottom", "↓")
            .add_with_icon('o', "Open fold", "▼")
            .add_with_icon('c', "Close fold", "▶")
            .add_with_icon('a', "Toggle fold", "⇄"),
        // [ prefix - Previous
        WhichKeyGroup::new('[', "Previous")
            .add_with_icon('e', "Error", "●")
            .add_with_icon('w', "Warning", "▲")
            .add_with_icon('d', "Diagnostic", "◆")
            .add_with_icon('h', "Hunk (git)", "±")
            .add_with_icon('c', "Change", "~"),
        // ] prefix - Next
        WhichKeyGroup::new(']', "Next")
            .add_with_icon('e', "Error", "●")
            .add_with_icon('w', "Warning", "▲")
            .add_with_icon('d', "Diagnostic", "◆")
            .add_with_icon('h', "Hunk (git)", "±")
            .add_with_icon('c', "Change", "~"),
        // Space (leader) prefix - Commands
        WhichKeyGroup::new(' ', "Leader")
            .add_with_icon('f', "Find file", "🔍")
            .add_with_icon('b', "Buffers", "📑")
            .add_with_icon('w', "Save", "💾")
            .add_with_icon('q', "Quit", "🚪")
            .add_with_icon('e', "Explorer", "📁")
            .add_with_icon('g', "Git", "±")
            .add_with_icon('s', "Search", "🔎")
            .add_with_icon('r', "Run", "▶"),
    ]
}

/// Which-key popup state
#[derive(Debug, Default)]
pub struct WhichKeyState {
    /// Currently active prefix (if any)
    active_prefix: Option<char>,
    /// When the prefix was pressed
    prefix_time: Option<Instant>,
    /// Whether the popup is visible
    visible: bool,
    /// Available groups
    groups: Vec<WhichKeyGroup>,
}

impl WhichKeyState {
    pub fn new() -> Self {
        Self {
            active_prefix: None,
            prefix_time: None,
            visible: false,
            groups: default_which_key_groups(),
        }
    }

    /// Set custom which-key groups
    pub fn with_groups(mut self, groups: Vec<WhichKeyGroup>) -> Self {
        self.groups = groups;
        self
    }

    /// Called when a potential prefix key is pressed
    pub fn on_prefix(&mut self, key: char) -> bool {
        // Check if this is a valid prefix
        if self.groups.iter().any(|g| g.prefix == key) {
            self.active_prefix = Some(key);
            self.prefix_time = Some(Instant::now());
            true
        } else {
            false
        }
    }

    /// Called on each tick to check if popup should appear
    pub fn tick(&mut self) {
        if let Some(prefix_time) = self.prefix_time {
            let elapsed = prefix_time.elapsed();

            // Show popup after delay
            if elapsed >= Duration::from_millis(POPUP_DELAY_MS) && !self.visible {
                self.visible = true;
            }

            // Auto-close after timeout
            if elapsed >= Duration::from_millis(POPUP_TIMEOUT_MS) {
                self.close();
            }
        }
    }

    /// Called when a follow-up key is pressed
    pub fn on_key(&mut self, key: char) -> Option<(char, char)> {
        if let Some(prefix) = self.active_prefix {
            // Find the binding
            if let Some(group) = self.groups.iter().find(|g| g.prefix == prefix) {
                if group.bindings.iter().any(|b| b.key == key) {
                    self.close();
                    return Some((prefix, key));
                }
            }
        }
        None
    }

    /// Close the popup
    pub fn close(&mut self) {
        self.active_prefix = None;
        self.prefix_time = None;
        self.visible = false;
    }

    /// Check if popup is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Check if a prefix is pending (but popup not yet visible)
    pub fn is_pending(&self) -> bool {
        self.active_prefix.is_some() && !self.visible
    }

    /// Get current prefix
    pub fn prefix(&self) -> Option<char> {
        self.active_prefix
    }

    /// Get current group (if any)
    pub fn current_group(&self) -> Option<&WhichKeyGroup> {
        self.active_prefix
            .and_then(|p| self.groups.iter().find(|g| g.prefix == p))
    }
}

/// Which-key popup widget
pub struct WhichKey<'a> {
    state: &'a WhichKeyState,
}

impl<'a> WhichKey<'a> {
    pub fn new(state: &'a WhichKeyState) -> Self {
        Self { state }
    }
}

impl Widget for WhichKey<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Only render if visible
        if !self.state.is_visible() {
            return;
        }

        let group = match self.state.current_group() {
            Some(g) => g,
            None => return,
        };

        // Calculate popup size
        let num_bindings = group.bindings.len();
        let cols = 2.min(num_bindings.div_ceil(4)); // 2 columns max
        let rows = num_bindings.div_ceil(cols);

        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = (rows as u16 + 4).min(area.height.saturating_sub(4));

        // Center the popup
        let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear background
        Clear.render(popup_area, buf);

        // Draw border
        let prefix_display = if group.prefix == ' ' {
            "Space".to_string()
        } else {
            group.prefix.to_string()
        };

        let block = Block::default()
            .title(format!(" {} ({}) ", group.name, prefix_display))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(compat::VIOLET_600))
            .style(Style::default().bg(compat::GRAY_900));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // Render bindings in columns
        let col_width = inner.width / 2;

        for (i, binding) in group.bindings.iter().enumerate() {
            let col = i % 2;
            let row = i / 2;

            if row as u16 >= inner.height {
                break;
            }

            let x = inner.x + (col as u16 * col_width);
            let y = inner.y + row as u16;

            // Key
            let key_display = format!("  {}  ", binding.key);
            buf.set_string(
                x,
                y,
                &key_display,
                Style::default()
                    .fg(compat::CYAN_500)
                    .add_modifier(Modifier::BOLD),
            );

            // Icon (if any)
            let icon_offset = UnicodeWidthStr::width(key_display.as_str()) as u16;
            if let Some(icon) = binding.icon {
                buf.set_string(
                    x + icon_offset,
                    y,
                    icon,
                    Style::default().fg(compat::SLATE_500),
                );
            }

            // Description
            let desc_offset = icon_offset + if binding.icon.is_some() { 2 } else { 0 };
            let max_desc_len = col_width.saturating_sub(desc_offset + 1) as usize;
            let description = if UnicodeWidthStr::width(binding.description) > max_desc_len {
                truncate_to_width(binding.description, max_desc_len.saturating_sub(1))
            } else {
                binding.description.to_string()
            };

            buf.set_string(
                x + desc_offset,
                y,
                &description,
                Style::default().fg(compat::SLATE_200),
            );
        }

        // Footer hint
        if inner.height > rows as u16 + 1 {
            let hint = "Press key or Esc to cancel";
            let hint_x = inner.x + (inner.width.saturating_sub(hint.len() as u16)) / 2;
            let hint_y = inner.y + inner.height - 1;
            buf.set_string(
                hint_x,
                hint_y,
                hint,
                Style::default()
                    .fg(compat::SLATE_500)
                    .add_modifier(Modifier::ITALIC),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_which_key_state_new() {
        let state = WhichKeyState::new();
        assert!(!state.is_visible());
        assert!(!state.is_pending());
        assert!(state.prefix().is_none());
    }

    #[test]
    fn test_which_key_on_prefix() {
        let mut state = WhichKeyState::new();

        // Valid prefix
        assert!(state.on_prefix('g'));
        assert!(state.is_pending());
        assert!(!state.is_visible());
        assert_eq!(state.prefix(), Some('g'));

        // Invalid prefix
        state.close();
        assert!(!state.on_prefix('x')); // Not a valid prefix
        assert!(!state.is_pending());
    }

    #[test]
    fn test_which_key_on_key() {
        let mut state = WhichKeyState::new();
        state.on_prefix('g');

        // Valid follow-up key
        let result = state.on_key('g');
        assert_eq!(result, Some(('g', 'g')));
        assert!(!state.is_visible());

        // State should be closed
        assert!(state.prefix().is_none());
    }

    #[test]
    fn test_which_key_close() {
        let mut state = WhichKeyState::new();
        state.on_prefix('z');
        state.visible = true;

        state.close();

        assert!(!state.is_visible());
        assert!(state.prefix().is_none());
    }

    #[test]
    fn test_which_key_current_group() {
        let mut state = WhichKeyState::new();

        // No prefix
        assert!(state.current_group().is_none());

        // With prefix
        state.on_prefix('g');
        let group = state.current_group();
        assert!(group.is_some());
        assert_eq!(group.unwrap().prefix, 'g');
    }

    #[test]
    fn test_which_key_group_builder() {
        let group = WhichKeyGroup::new('t', "Test")
            .add('a', "Action A")
            .add_with_icon('b', "Action B", "★");

        assert_eq!(group.prefix, 't');
        assert_eq!(group.bindings.len(), 2);
        assert_eq!(group.bindings[0].key, 'a');
        assert_eq!(group.bindings[1].icon, Some("★"));
    }

    #[test]
    fn test_default_groups_exist() {
        let groups = default_which_key_groups();

        // Should have g, z, [, ], and space groups
        assert!(groups.iter().any(|g| g.prefix == 'g'));
        assert!(groups.iter().any(|g| g.prefix == 'z'));
        assert!(groups.iter().any(|g| g.prefix == '['));
        assert!(groups.iter().any(|g| g.prefix == ']'));
        assert!(groups.iter().any(|g| g.prefix == ' '));
    }

    #[test]
    fn test_which_key_space_leader() {
        let mut state = WhichKeyState::new();

        // Space as leader key
        assert!(state.on_prefix(' '));

        let group = state.current_group();
        assert!(group.is_some());
        assert_eq!(group.unwrap().name, "Leader");
    }
}
