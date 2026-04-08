// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Layout Calculations for Chat View
//!
//! Panel area computation and hit-testing helpers for the chat view.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

// ═══════════════════════════════════════════════════════════════════════════════
// Panel Area Computation
// ═══════════════════════════════════════════════════════════════════════════════

/// Compute panel areas from total area (same layout as render).
/// Returns (session_bar, conversation, activity, input, hints) areas.
///
/// This must match the layout in ChatView::render() exactly for mouse
/// hit-testing to work correctly.
///
/// `warning_height` should be 1 when no API key is configured, 0 otherwise.
/// `input_height` should match `ChatView::calculate_input_height()`.
pub fn compute_panel_areas(
    area: Rect,
    warning_height: u16,
    input_height: u16,
) -> (Rect, Rect, Rect, Rect, Rect) {
    // Vertical layout must match render() exactly:
    // ProStatusBar (2) | Warning (0 or 1) | main content | input (dynamic) | hints (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),              // ProStatusBar (2 lines)
            Constraint::Length(warning_height), // API key warning (0 or 1)
            Constraint::Min(10),                // Main content area
            Constraint::Length(input_height),   // Input field (dynamic)
            Constraint::Length(1),              // Command hints
        ])
        .split(area);

    // Horizontal split must match render() - 65%/35%
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(chunks[2]);

    (
        chunks[0],
        main_chunks[0],
        main_chunks[1],
        chunks[3],
        chunks[4],
    )
}

// ═══════════════════════════════════════════════════════════════════════════════
// Hit Testing
// ═══════════════════════════════════════════════════════════════════════════════

/// Check if a point is inside a rect.
/// Used for mouse click detection in panel areas.
#[inline]
pub fn point_in_rect(x: u16, y: u16, rect: Rect) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_panel_areas_splits_correctly() {
        let area = Rect::new(0, 0, 100, 50);
        let (session, conversation, activity, input, hints) = compute_panel_areas(area, 0, 3);

        // Session bar at top (2 lines)
        assert_eq!(session.height, 2);
        assert_eq!(session.y, 0);

        // Input field (3 lines)
        assert_eq!(input.height, 3);

        // Hints at bottom (1 line)
        assert_eq!(hints.height, 1);
        assert_eq!(hints.y, 49); // 50 - 1

        // Conversation and activity split 65/35 horizontally
        assert_eq!(conversation.x, 0);
        assert_eq!(conversation.width, 65); // 65% of 100
        assert_eq!(activity.x, 65);
        assert_eq!(activity.width, 35); // 35% of 100
    }

    #[test]
    fn test_compute_panel_areas_with_warning() {
        let area = Rect::new(0, 0, 100, 50);
        let (session, _conversation, _activity, input, hints) = compute_panel_areas(area, 1, 3);

        // Session bar at top (2 lines)
        assert_eq!(session.height, 2);

        // Warning bar takes 1 line, pushing content down
        assert_eq!(input.height, 3);
        assert_eq!(hints.height, 1);
        assert_eq!(hints.y, 49);
    }

    #[test]
    fn test_point_in_rect_inside() {
        let rect = Rect::new(10, 20, 30, 40);

        // Inside
        assert!(point_in_rect(10, 20, rect)); // Top-left corner
        assert!(point_in_rect(25, 40, rect)); // Middle
        assert!(point_in_rect(39, 59, rect)); // Just inside bottom-right
    }

    #[test]
    fn test_point_in_rect_outside() {
        let rect = Rect::new(10, 20, 30, 40);

        // Outside
        assert!(!point_in_rect(9, 20, rect)); // Left of rect
        assert!(!point_in_rect(10, 19, rect)); // Above rect
        assert!(!point_in_rect(40, 20, rect)); // Right edge (exclusive)
        assert!(!point_in_rect(10, 60, rect)); // Bottom edge (exclusive)
    }

    #[test]
    fn test_point_in_rect_zero_area() {
        let rect = Rect::new(10, 10, 0, 0);

        // Zero-sized rect contains nothing
        assert!(!point_in_rect(10, 10, rect));
    }

    #[test]
    fn test_panel_areas_small_terminal() {
        // Minimum viable terminal size
        let area = Rect::new(0, 0, 80, 24);
        let (session, conversation, activity, input, hints) = compute_panel_areas(area, 0, 3);

        // All panels should have valid dimensions
        assert!(session.height > 0);
        assert!(conversation.width > 0);
        assert!(activity.width > 0);
        assert!(input.height > 0);
        assert!(hints.height > 0);

        // Total height should not exceed area
        let total_height = session.height + input.height + hints.height;
        assert!(total_height <= area.height);
    }
}
