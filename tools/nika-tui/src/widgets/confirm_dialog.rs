// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Confirmation Dialog — Modal popup for destructive actions
//!
//! Renders a centered popup with a warning message and [Y]es / [N]o options.
//! Used before irreversible operations like clearing chat history.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::tokens::compat;

// ═══════════════════════════════════════════════════════════════════════════════
// Confirm Action Enum
// ═══════════════════════════════════════════════════════════════════════════════

/// Destructive actions that require user confirmation before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    /// Clear all chat messages and inline content
    ClearChat,
}

impl ConfirmAction {
    /// Warning message displayed in the dialog
    pub fn message(&self) -> &'static str {
        match self {
            ConfirmAction::ClearChat => "Clear all chat history?",
        }
    }

    /// Detailed description shown below the main message
    pub fn detail(&self) -> &'static str {
        match self {
            ConfirmAction::ClearChat => {
                "This will remove all messages, inline content, and activity items. This cannot be undone."
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Confirm Dialog Widget
// ═══════════════════════════════════════════════════════════════════════════════

/// Stateless widget that renders a confirmation dialog overlay.
///
/// Displays a centered popup with:
/// - Warning icon and action message
/// - Detail text explaining consequences
/// - \[Y\]es / \[N\]o key hints
pub struct ConfirmDialog<'a> {
    action: &'a ConfirmAction,
}

impl<'a> ConfirmDialog<'a> {
    pub fn new(action: &'a ConfirmAction) -> Self {
        Self { action }
    }
}

impl Widget for ConfirmDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Semi-transparent background overlay (darken everything behind)
        let bg_style = Style::default().bg(Color::Rgb(0, 0, 0));
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(bg_style);
                    cell.set_char(' ');
                }
            }
        }

        // Dialog box dimensions: compact centered box
        let box_width = 50.min(area.width.saturating_sub(4));
        let box_height = 9.min(area.height.saturating_sub(4));
        let box_x = area.x + (area.width.saturating_sub(box_width)) / 2;
        let box_y = area.y + (area.height.saturating_sub(box_height)) / 2;
        let box_area = Rect::new(box_x, box_y, box_width, box_height);

        // Clear the box area
        Clear.render(box_area, buf);

        // Border with warning-colored border (amber)
        let block = Block::default()
            .title(" Confirm ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(compat::AMBER_500))
            .style(Style::default().bg(compat::GRAY_900));

        let inner = block.inner(box_area);
        block.render(box_area, buf);

        // Build content lines
        let lines = vec![
            Line::from(""),
            // Warning icon + message
            Line::from(vec![
                Span::styled(
                    "  ⚠ ",
                    Style::default()
                        .fg(compat::AMBER_400)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    self.action.message(),
                    Style::default()
                        .fg(compat::SLATE_100)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            // Detail text (muted)
            Line::from(Span::styled(
                format!("  {}", self.action.detail()),
                Style::default().fg(compat::SLATE_400),
            )),
            Line::from(""),
            // Key hints
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    " Y ",
                    Style::default()
                        .fg(compat::GRAY_900)
                        .bg(compat::RED_500)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Confirm   ", Style::default().fg(compat::SLATE_300)),
                Span::styled(
                    " N ",
                    Style::default()
                        .fg(compat::GRAY_900)
                        .bg(compat::SLATE_500)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Cancel", Style::default().fg(compat::SLATE_300)),
            ]),
        ];

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        paragraph.render(inner, buf);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirm_action_messages() {
        assert_eq!(
            ConfirmAction::ClearChat.message(),
            "Clear all chat history?"
        );
        assert!(!ConfirmAction::ClearChat.detail().is_empty());
    }

    #[test]
    fn test_confirm_action_eq() {
        assert_eq!(ConfirmAction::ClearChat, ConfirmAction::ClearChat);
    }

    #[test]
    fn test_confirm_dialog_renders_without_panic() {
        let action = ConfirmAction::ClearChat;
        let dialog = ConfirmDialog::new(&action);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        dialog.render(area, &mut buf);
        // Verify the dialog rendered something (box area is not all spaces)
        let center_y = 12; // Approximate center
        let center_x = 40;
        // Just verify no panic occurred — the widget rendered successfully
        let _ = buf.cell((center_x, center_y));
    }
}
