// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Help Overlay — Keyboard shortcuts reference
//!
//! Shows all keybindings organized by context.
//! Toggle with ? or F1, scroll with j/k, close with Escape.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::tokens::compat;

/// A section of keybindings
#[derive(Debug, Clone)]
pub struct HelpSection {
    pub title: &'static str,
    pub keybindings: &'static [(&'static str, &'static str)],
}

/// All help sections for the TUI
pub const HELP_SECTIONS: &[HelpSection] = &[
    HelpSection {
        title: "Navigation",
        keybindings: &[
            ("Tab / Shift+Tab", "Cycle views"),
            ("1 / s", "Studio"),
            ("2 / c", "Command"),
            ("3 / x", "Control"),
            ("Ctrl+P", "Provider select (Chat) / Fuzzy search (Studio)"),
            ("Ctrl+F", "Search in conversation"),
            ("?  or  F1", "Toggle help"),
            ("Escape", "Close overlay / Cancel"),
        ],
    },
    HelpSection {
        title: "Command View (Chat Mode)",
        keybindings: &[
            ("Enter", "Send message"),
            ("Ctrl+Enter", "New line"),
            ("Up / Down", "Input history"),
            ("j / k", "Scroll messages"),
            ("g / G", "Top / Bottom"),
            ("Ctrl+P", "Provider / model selector"),
            ("Ctrl+F", "Search conversation"),
            ("Ctrl+M", "Toggle Chat/Monitor mode"),
            ("y", "Copy last response"),
            ("Y", "Copy without markdown"),
            ("/infer /exec /fetch", "Verb prefixes"),
            ("/invoke /agent", "MCP / Agent verbs"),
        ],
    },
    HelpSection {
        title: "Studio View",
        keybindings: &[
            ("Tab", "Next panel (Browser/Editor/DAG)"),
            ("Shift+Tab", "Previous panel"),
            ("Ctrl+]", "Cycle panel ratio"),
            ("j / k", "Navigate files / scroll"),
            ("Enter", "Open workflow"),
            ("Space", "Preview file"),
            ("/ or Ctrl+P", "Fuzzy file search"),
            ("r", "Refresh list"),
            ("n", "New workflow"),
            ("Ctrl+S", "Save file"),
            ("Ctrl+Z", "Undo"),
            ("Ctrl+Y", "Redo"),
            ("F5", "Run workflow"),
            ("F6", "Validate"),
        ],
    },
    HelpSection {
        title: "Command View (Monitor Mode)",
        keybindings: &[
            ("j / k", "Navigate tasks"),
            ("y", "Copy task output to clipboard"),
            ("t", "Cycle sub-tab"),
            ("Enter", "Expand task detail"),
            ("Space", "Toggle section"),
            ("Ctrl+M", "Toggle Chat/Monitor mode"),
        ],
    },
    HelpSection {
        title: "Text Input",
        keybindings: &[
            ("Ctrl+A", "Select all"),
            ("Ctrl+K", "Delete to end"),
            ("Ctrl+U", "Delete to start"),
            ("Ctrl+W", "Delete word"),
            ("Alt+Left/Right", "Word jump"),
            ("Home / End", "Line start/end"),
        ],
    },
    HelpSection {
        title: "General",
        keybindings: &[
            ("Ctrl+C", "Interrupt / Quit"),
            ("Ctrl+L", "Clear screen"),
            ("Ctrl+R", "Reload config"),
        ],
    },
];

/// Help overlay state
#[derive(Debug, Default)]
pub struct HelpOverlayState {
    /// Is overlay visible
    pub visible: bool,
    /// Current scroll position
    pub scroll: usize,
    /// Search query (optional)
    pub search: Option<String>,
    /// PERF: Cached rendered content lines (built once, reused every frame)
    cached_content: Option<Vec<Line<'static>>>,
}

impl HelpOverlayState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.scroll = 0;
            self.search = None;
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.scroll = 0;
        self.search = None;
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, max_scroll: usize) {
        if self.scroll < max_scroll {
            self.scroll += 1;
        }
    }

    pub fn scroll_page_up(&mut self, page_size: usize) {
        self.scroll = self.scroll.saturating_sub(page_size);
    }

    pub fn scroll_page_down(&mut self, max_scroll: usize, page_size: usize) {
        self.scroll = (self.scroll + page_size).min(max_scroll);
    }

    /// Calculate total content height from HELP_SECTIONS
    ///
    /// Each section contributes: 1 title line + keybinding count + 1 trailing blank line.
    pub fn content_height(&self) -> usize {
        let mut height = 0;
        for section in HELP_SECTIONS {
            height += 2; // Title + blank line
            height += section.keybindings.len();
            height += 1; // Spacing after section
        }
        height
    }
}

/// Help overlay widget
pub struct HelpOverlay<'a> {
    state: &'a mut HelpOverlayState,
}

impl<'a> HelpOverlay<'a> {
    pub fn new(state: &'a mut HelpOverlayState) -> Self {
        // PERF: Ensure content is cached on first use (built once, never changes)
        if state.cached_content.is_none() {
            state.cached_content = Some(Self::build_content_static());
        }
        Self { state }
    }

    /// Build content lines (static — only called once, then cached)
    fn build_content_static() -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        for section in HELP_SECTIONS {
            // Section title
            lines.push(Line::from(vec![Span::styled(
                format!(" {} ", section.title),
                Style::default()
                    .fg(compat::AMBER_500)
                    .add_modifier(Modifier::BOLD),
            )]));

            // Keybindings
            for (key, desc) in section.keybindings.iter() {
                let key_width = 20;
                let padded_key = format!("  {:<width$}", key, width = key_width);

                lines.push(Line::from(vec![
                    Span::styled(padded_key, Style::default().fg(compat::CYAN_500)),
                    Span::styled(*desc, Style::default().fg(compat::SLATE_200)),
                ]));
            }

            // Blank line after section
            lines.push(Line::from(""));
        }

        lines
    }
}

impl Widget for HelpOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Semi-transparent background overlay
        let bg_style = Style::default().bg(Color::Rgb(0, 0, 0));
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    // Darken existing content
                    cell.set_style(bg_style);
                    cell.set_char(' ');
                }
            }
        }

        // Calculate centered box dimensions
        let box_width = 60.min(area.width.saturating_sub(4));
        let box_height = (area.height.saturating_sub(4)).min(35);
        let box_x = area.x + (area.width.saturating_sub(box_width)) / 2;
        let box_y = area.y + (area.height.saturating_sub(box_height)) / 2;
        let box_area = Rect::new(box_x, box_y, box_width, box_height);

        // Clear the box area
        Clear.render(box_area, buf);

        // Box with border
        let block = Block::default()
            .title(" Keyboard Shortcuts ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(compat::VIOLET_600))
            .style(Style::default().bg(compat::GRAY_900));

        let inner = block.inner(box_area);
        block.render(box_area, buf);

        // PERF: Use cached content (built once in new(), never changes)
        let content = self.state.cached_content.as_ref().unwrap();
        let content_height = content.len();

        // Apply scroll — clone only the visible slice
        let visible_lines: Vec<Line> = content
            .iter()
            .skip(self.state.scroll)
            .take(inner.height as usize)
            .cloned()
            .collect();

        let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
        paragraph.render(inner, buf);

        // Scroll indicator
        if content_height > inner.height as usize {
            let max_scroll = content_height.saturating_sub(inner.height as usize);
            let scroll_percentage = if max_scroll > 0 {
                (self.state.scroll as f32 / max_scroll as f32 * 100.0) as u16
            } else {
                0
            };

            let scroll_info = format!(
                " {}/{} ({}%) ",
                self.state.scroll + 1,
                content_height,
                scroll_percentage
            );

            // Bottom right of box
            let info_x = box_area
                .right()
                .saturating_sub(scroll_info.len() as u16 + 1);
            let info_y = box_area.bottom().saturating_sub(1);

            for (i, ch) in scroll_info.chars().enumerate() {
                if let Some(cell) = buf.cell_mut((info_x + i as u16, info_y)) {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(compat::SLATE_500));
                }
            }
        }

        // Help hint at bottom
        let hint = " j/k scroll | Esc close ";
        let hint_x = box_x + (box_width.saturating_sub(hint.len() as u16)) / 2;
        let hint_y = box_area.bottom().saturating_sub(1);

        for (i, ch) in hint.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((hint_x + i as u16, hint_y)) {
                cell.set_char(ch);
                cell.set_style(
                    Style::default()
                        .fg(compat::SLATE_500)
                        .add_modifier(Modifier::DIM),
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_overlay_state_toggle() {
        let mut state = HelpOverlayState::new();
        assert!(!state.visible);

        state.toggle();
        assert!(state.visible);

        state.toggle();
        assert!(!state.visible);
    }

    #[test]
    fn test_help_overlay_state_show_hide() {
        let mut state = HelpOverlayState::new();

        state.show();
        assert!(state.visible);

        state.hide();
        assert!(!state.visible);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn test_help_overlay_state_scroll() {
        let mut state = HelpOverlayState::new();
        state.show();

        state.scroll_down(10);
        assert_eq!(state.scroll, 1);

        state.scroll_down(10);
        assert_eq!(state.scroll, 2);

        state.scroll_up();
        assert_eq!(state.scroll, 1);

        state.scroll_up();
        state.scroll_up(); // Should not go below 0
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn test_help_overlay_state_page_scroll() {
        let mut state = HelpOverlayState::new();
        state.show();

        state.scroll_page_down(100, 10);
        assert_eq!(state.scroll, 10);

        state.scroll_page_up(5);
        assert_eq!(state.scroll, 5);
    }

    #[test]
    fn test_help_sections_not_empty() {
        assert!(!HELP_SECTIONS.is_empty());

        for section in HELP_SECTIONS {
            assert!(!section.title.is_empty());
            assert!(!section.keybindings.is_empty());
        }
    }

    #[test]
    fn test_help_overlay_build_content() {
        let mut state = HelpOverlayState::new();
        let overlay = HelpOverlay::new(&mut state);

        // Content is cached in state after new()
        assert!(overlay.state.cached_content.is_some());
        assert!(!overlay.state.cached_content.as_ref().unwrap().is_empty());
    }
}
