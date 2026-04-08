// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Terminal Size Handling — graceful degradation for small terminals
//!
//! Provides minimum size checking and a fallback overlay when terminal is too small.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap},
};

use crate::tokens::compat;

/// Minimum terminal dimensions for proper rendering
pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 15;

/// Compact mode threshold (hide activity panel)
pub const COMPACT_WIDTH: u16 = 80;

/// Wide mode threshold (side-by-side layout)
pub const WIDE_WIDTH: u16 = 120;

/// Layout mode based on terminal width
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Terminal too small — show error overlay
    TooSmall,
    /// 60-80 columns — hide Activity panel
    Compact,
    /// 80-120 columns — standard 3-panel layout
    Standard,
    /// 120+ columns — wide layout with extra space
    Wide,
}

impl LayoutMode {
    /// Determine layout mode from terminal size
    pub fn from_size(width: u16, height: u16) -> Self {
        if width < MIN_WIDTH || height < MIN_HEIGHT {
            Self::TooSmall
        } else if width < COMPACT_WIDTH {
            Self::Compact
        } else if width < WIDE_WIDTH {
            Self::Standard
        } else {
            Self::Wide
        }
    }

    /// Check if Activity panel should be shown
    pub fn show_activity(&self) -> bool {
        matches!(self, Self::Standard | Self::Wide)
    }

    /// Check if terminal is usable
    pub fn is_usable(&self) -> bool {
        !matches!(self, Self::TooSmall)
    }
}

/// Widget to display when terminal is too small
pub struct TerminalTooSmallOverlay {
    current_width: u16,
    current_height: u16,
}

impl TerminalTooSmallOverlay {
    pub fn new(current_width: u16, current_height: u16) -> Self {
        Self {
            current_width,
            current_height,
        }
    }
}

impl Widget for TerminalTooSmallOverlay {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear background with dark color
        let bg_style = Style::default().bg(compat::GRAY_900);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(bg_style);
                }
            }
        }

        // Center the message box
        let box_width = 40.min(area.width.saturating_sub(4));
        let box_height = 11.min(area.height.saturating_sub(2));
        let box_x = area.x + (area.width.saturating_sub(box_width)) / 2;
        let box_y = area.y + (area.height.saturating_sub(box_height)) / 2;
        let box_area = Rect::new(box_x, box_y, box_width, box_height);

        // Warning icon and title
        let title = " ⚠️  Terminal Too Small ";
        let block = Block::default()
            .title(title)
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(compat::SLATE_800));

        let inner = block.inner(box_area);
        block.render(box_area, buf);

        // Content layout
        let content_chunks = Layout::default()
            .constraints([
                Constraint::Length(1), // Spacer
                Constraint::Length(2), // Size info
                Constraint::Length(1), // Spacer
                Constraint::Length(2), // Minimum required
                Constraint::Length(1), // Spacer
                Constraint::Min(1),    // Hint
            ])
            .split(inner);

        // Current size
        let current_text = vec![Line::from(vec![
            Span::styled("Current: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}×{}", self.current_width, self.current_height),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ])];
        Paragraph::new(current_text)
            .alignment(Alignment::Center)
            .render(content_chunks[1], buf);

        // Required size
        let required_text = vec![Line::from(vec![
            Span::styled("Minimum: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}×{}", MIN_WIDTH, MIN_HEIGHT),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];
        Paragraph::new(required_text)
            .alignment(Alignment::Center)
            .render(content_chunks[3], buf);

        // Hint
        let hint = Paragraph::new(Line::from(vec![Span::styled(
            "Please resize your terminal",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )]))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        hint.render(content_chunks[5], buf);
    }
}

/// Helper to check terminal size and return layout mode
pub fn check_terminal_size(area: Rect) -> LayoutMode {
    LayoutMode::from_size(area.width, area.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_mode_too_small() {
        assert_eq!(LayoutMode::from_size(50, 10), LayoutMode::TooSmall);
        assert_eq!(LayoutMode::from_size(60, 10), LayoutMode::TooSmall);
        assert_eq!(LayoutMode::from_size(50, 15), LayoutMode::TooSmall);
    }

    #[test]
    fn test_layout_mode_compact() {
        assert_eq!(LayoutMode::from_size(60, 15), LayoutMode::Compact);
        assert_eq!(LayoutMode::from_size(79, 20), LayoutMode::Compact);
    }

    #[test]
    fn test_layout_mode_standard() {
        assert_eq!(LayoutMode::from_size(80, 20), LayoutMode::Standard);
        assert_eq!(LayoutMode::from_size(119, 30), LayoutMode::Standard);
    }

    #[test]
    fn test_layout_mode_wide() {
        assert_eq!(LayoutMode::from_size(120, 30), LayoutMode::Wide);
        assert_eq!(LayoutMode::from_size(200, 50), LayoutMode::Wide);
    }

    #[test]
    fn test_show_activity() {
        assert!(!LayoutMode::TooSmall.show_activity());
        assert!(!LayoutMode::Compact.show_activity());
        assert!(LayoutMode::Standard.show_activity());
        assert!(LayoutMode::Wide.show_activity());
    }

    #[test]
    fn test_is_usable() {
        assert!(!LayoutMode::TooSmall.is_usable());
        assert!(LayoutMode::Compact.is_usable());
        assert!(LayoutMode::Standard.is_usable());
        assert!(LayoutMode::Wide.is_usable());
    }
}
