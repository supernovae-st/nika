// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Panel Scroll State Management

use std::ops::Range;

/// Scroll margin for cursor visibility (lines from edge)
pub const SCROLL_MARGIN: usize = 3;

/// Scroll state for a panel with cursor/scroll separation
///
/// Pattern from NovaNet TUI: cursor position is independent of scroll offset.
/// `ensure_cursor_visible()` adjusts scroll to keep cursor in view with margin.
#[derive(Debug, Clone, Default)]
pub struct PanelScrollState {
    /// Current scroll offset (0-indexed, first visible line)
    pub offset: usize,
    /// Cursor position (0-indexed, selected item)
    pub cursor: usize,
    /// Total number of items
    pub total: usize,
    /// Visible items count (viewport height)
    pub visible: usize,
}

impl PanelScrollState {
    /// Create new scroll state
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with initial total count
    pub fn with_total(total: usize) -> Self {
        Self {
            total,
            ..Default::default()
        }
    }

    /// Ensure cursor is visible by adjusting scroll offset
    ///
    /// Maintains a margin of SCROLL_MARGIN lines from top/bottom edges.
    /// This prevents the cursor from being at the very edge of the viewport.
    pub fn ensure_cursor_visible(&mut self) {
        if self.visible == 0 || self.total == 0 {
            return;
        }

        // Calculate effective margin (can't be more than half the viewport)
        let margin = SCROLL_MARGIN.min(self.visible / 2);

        // Cursor above visible area (with margin)
        if self.cursor < self.offset.saturating_add(margin) {
            self.offset = self.cursor.saturating_sub(margin);
        }

        // Cursor below visible area (with margin)
        let bottom_threshold = self.offset + self.visible.saturating_sub(margin);
        if self.cursor >= bottom_threshold && self.total > self.visible {
            self.offset = (self.cursor + margin + 1).saturating_sub(self.visible);
            // Clamp to max scroll
            let max_offset = self.total.saturating_sub(self.visible);
            self.offset = self.offset.min(max_offset);
        }
    }

    /// Move cursor down by one item
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.total {
            self.cursor += 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move cursor up by one item
    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move cursor to first item
    pub fn cursor_first(&mut self) {
        self.cursor = 0;
        self.ensure_cursor_visible();
    }

    /// Move cursor to last item
    pub fn cursor_last(&mut self) {
        if self.total > 0 {
            self.cursor = self.total - 1;
            self.ensure_cursor_visible();
        }
    }

    /// Page down (move cursor by visible count)
    pub fn page_down(&mut self) {
        if self.total > 0 {
            self.cursor = (self.cursor + self.visible).min(self.total - 1);
            self.ensure_cursor_visible();
        }
    }

    /// Page up (move cursor by visible count)
    pub fn page_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(self.visible);
        self.ensure_cursor_visible();
    }

    /// Scroll down by one item
    /// NovaNet pattern - scroll works before render sets `visible`
    pub fn scroll_down(&mut self) {
        // Calculate max offset:
        // - If visible is known (> 0), cap at total - visible
        // - Otherwise, allow scrolling to total - 1 (will be clamped at render)
        let max_offset = if self.visible > 0 {
            self.total.saturating_sub(self.visible)
        } else {
            self.total.saturating_sub(1)
        };

        if self.offset < max_offset {
            self.offset += 1;
        }
    }

    /// Scroll up by one item
    /// NovaNet pattern - scroll works before render sets `visible`
    pub fn scroll_up(&mut self) {
        if self.offset > 0 {
            self.offset -= 1;
        }
    }

    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        self.offset = 0;
        self.cursor = 0;
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        if self.total > self.visible {
            self.offset = self.total - self.visible;
        } else {
            self.offset = 0;
        }
        if self.total > 0 {
            self.cursor = self.total - 1;
        }
    }

    /// Update total items count
    pub fn set_total(&mut self, total: usize) {
        self.total = total;
        // Clamp cursor to valid range
        if self.total > 0 && self.cursor >= self.total {
            self.cursor = self.total - 1;
        }
        // Adjust offset if needed
        if self.total > 0 && self.offset + self.visible > self.total {
            self.offset = self.total.saturating_sub(self.visible);
        }
    }

    /// Update visible items count (viewport height)
    pub fn set_visible(&mut self, visible: usize) {
        self.visible = visible;
        self.ensure_cursor_visible();
    }

    /// Get selected item index (alias for cursor)
    pub fn selected(&self) -> Option<usize> {
        if self.total > 0 {
            Some(self.cursor)
        } else {
            None
        }
    }

    /// Check if item at index is the cursor position
    pub fn is_selected(&self, index: usize) -> bool {
        self.cursor == index
    }

    /// Get scroll percentage (0.0 - 1.0)
    pub fn percentage(&self) -> f64 {
        if self.total <= self.visible {
            0.0
        } else {
            self.offset as f64 / (self.total - self.visible) as f64
        }
    }

    /// Get visible range [start, end) of items
    pub fn visible_range(&self) -> Range<usize> {
        let start = self.offset;
        let end = (self.offset + self.visible).min(self.total);
        start..end
    }

    /// Check if scroll is at top
    pub fn at_top(&self) -> bool {
        self.offset == 0
    }

    /// Check if scroll is at bottom
    pub fn at_bottom(&self) -> bool {
        self.total <= self.visible || self.offset >= self.total - self.visible
    }

    /// Compact scroll indicator for panel titles
    ///
    /// Returns `None` if content fits in viewport (no scroll needed).
    /// Format: `" ↕ 45% "` or `" ↑ Bot "` with percentage/position:
    /// - `Top` when at top
    /// - `Bot` when at bottom
    /// - `XX%` in middle (scroll percentage)
    ///
    /// Directional arrow:
    /// - `↓` at top (can scroll down)
    /// - `↑` at bottom (can scroll up)
    /// - `↕` in middle (can scroll both ways)
    pub fn indicator(&self) -> Option<String> {
        if self.total <= self.visible {
            return None;
        }

        let arrow = if self.at_top() {
            "↓"
        } else if self.at_bottom() {
            "↑"
        } else {
            "↕"
        };

        // Show position: Top, Bot, or percentage
        let position = if self.at_top() {
            "Top".to_string()
        } else if self.at_bottom() {
            "Bot".to_string()
        } else {
            // Calculate percentage (0-100)
            let pct = (self.percentage() * 100.0).round() as u8;
            format!("{}%", pct)
        };

        Some(format!(" {} {} ", arrow, position))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scroll(total: usize, visible: usize) -> PanelScrollState {
        PanelScrollState {
            offset: 0,
            cursor: 0,
            total,
            visible,
        }
    }

    // ── cursor_down ──────────────────────────────────────────────────────

    #[test]
    fn cursor_down_basic() {
        let mut s = scroll(10, 5);
        s.cursor_down();
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn cursor_down_stops_at_last_item() {
        let mut s = scroll(3, 5);
        s.cursor = 2;
        s.cursor_down();
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn cursor_down_with_zero_items_is_noop() {
        let mut s = scroll(0, 5);
        s.cursor_down();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn cursor_down_scrolls_when_hitting_margin() {
        let mut s = scroll(20, 10);
        for _ in 0..7 {
            s.cursor_down();
        }
        assert_eq!(s.cursor, 7);
        assert!(s.offset > 0);
    }

    #[test]
    fn cursor_down_various_viewport_sizes() {
        let mut s = scroll(100, 3);
        for _ in 0..5 {
            s.cursor_down();
        }
        assert_eq!(s.cursor, 5);
        assert!(s.offset > 0);

        let mut s2 = scroll(5, 20);
        for _ in 0..10 {
            s2.cursor_down();
        }
        assert_eq!(s2.cursor, 4);
        assert_eq!(s2.offset, 0);
    }

    // ── cursor_up ────────────────────────────────────────────────────────

    #[test]
    fn cursor_up_at_zero_is_noop() {
        let mut s = scroll(10, 5);
        s.cursor_up();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn cursor_up_from_middle() {
        let mut s = scroll(10, 5);
        s.cursor = 5;
        s.cursor_up();
        assert_eq!(s.cursor, 4);
    }

    // ── page_down ────────────────────────────────────────────────────────

    #[test]
    fn page_down_basic() {
        let mut s = scroll(100, 10);
        s.page_down();
        assert_eq!(s.cursor, 10);
    }

    #[test]
    fn page_down_near_bottom_clamps() {
        let mut s = scroll(15, 10);
        s.cursor = 10;
        s.page_down();
        assert_eq!(s.cursor, 14);
    }

    #[test]
    fn page_down_with_zero_items_is_noop() {
        let mut s = scroll(0, 10);
        s.page_down();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn page_down_from_near_end() {
        let mut s = scroll(20, 10);
        s.cursor = 18;
        s.page_down();
        assert_eq!(s.cursor, 19);
    }

    // ── page_up ──────────────────────────────────────────────────────────

    #[test]
    fn page_up_at_top_is_noop() {
        let mut s = scroll(100, 10);
        s.page_up();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn page_up_from_middle() {
        let mut s = scroll(100, 10);
        s.cursor = 25;
        s.page_up();
        assert_eq!(s.cursor, 15);
    }

    #[test]
    fn page_up_near_top_saturates() {
        let mut s = scroll(100, 10);
        s.cursor = 3;
        s.page_up();
        assert_eq!(s.cursor, 0);
    }

    // ── scroll_to_bottom ─────────────────────────────────────────────────

    #[test]
    fn scroll_to_bottom_with_zero_items() {
        let mut s = scroll(0, 10);
        s.scroll_to_bottom();
        assert_eq!(s.offset, 0);
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn scroll_to_bottom_content_fits_viewport() {
        let mut s = scroll(5, 10);
        s.scroll_to_bottom();
        assert_eq!(s.offset, 0);
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn scroll_to_bottom_content_exceeds_viewport() {
        let mut s = scroll(50, 10);
        s.scroll_to_bottom();
        assert_eq!(s.offset, 40);
        assert_eq!(s.cursor, 49);
    }

    // ── scroll_to_top ────────────────────────────────────────────────────

    #[test]
    fn scroll_to_top_resets() {
        let mut s = scroll(50, 10);
        s.offset = 20;
        s.cursor = 25;
        s.scroll_to_top();
        assert_eq!(s.offset, 0);
        assert_eq!(s.cursor, 0);
    }

    // ── ensure_cursor_visible ────────────────────────────────────────────

    #[test]
    fn ensure_cursor_visible_cursor_above_viewport() {
        let mut s = scroll(50, 10);
        s.offset = 20;
        s.cursor = 5;
        s.ensure_cursor_visible();
        let range = s.visible_range();
        assert!(
            s.cursor >= range.start && s.cursor < range.end,
            "cursor {} should be in range {:?}",
            s.cursor,
            range
        );
    }

    #[test]
    fn ensure_cursor_visible_cursor_below_viewport() {
        let mut s = scroll(50, 10);
        s.offset = 0;
        s.cursor = 30;
        s.ensure_cursor_visible();
        let range = s.visible_range();
        assert!(
            s.cursor >= range.start && s.cursor < range.end,
            "cursor {} should be in range {:?}",
            s.cursor,
            range
        );
    }

    #[test]
    fn ensure_cursor_visible_zero_visible_is_noop() {
        let mut s = scroll(10, 0);
        s.cursor = 5;
        s.ensure_cursor_visible();
        assert_eq!(s.offset, 0);
    }

    #[test]
    fn ensure_cursor_visible_zero_total_is_noop() {
        let mut s = scroll(0, 10);
        s.ensure_cursor_visible();
        assert_eq!(s.offset, 0);
    }

    #[test]
    fn ensure_cursor_visible_already_in_view() {
        let mut s = scroll(50, 10);
        s.offset = 10;
        s.cursor = 15;
        let offset_before = s.offset;
        s.ensure_cursor_visible();
        assert_eq!(s.offset, offset_before);
    }

    // ── visible_range ────────────────────────────────────────────────────

    #[test]
    fn visible_range_basic() {
        let s = PanelScrollState {
            offset: 5,
            cursor: 5,
            total: 20,
            visible: 10,
        };
        assert_eq!(s.visible_range(), 5..15);
    }

    #[test]
    fn visible_range_clamped_to_total() {
        let s = PanelScrollState {
            offset: 15,
            cursor: 15,
            total: 20,
            visible: 10,
        };
        assert_eq!(s.visible_range(), 15..20);
    }

    #[test]
    fn visible_range_empty() {
        let s = scroll(0, 10);
        assert_eq!(s.visible_range(), 0..0);
    }

    #[test]
    fn visible_range_viewport_larger_than_total() {
        let s = scroll(5, 20);
        assert_eq!(s.visible_range(), 0..5);
    }

    // ── percentage ───────────────────────────────────────────────────────

    #[test]
    fn percentage_at_top_is_zero() {
        let s = scroll(50, 10);
        assert_eq!(s.percentage(), 0.0);
    }

    #[test]
    fn percentage_at_bottom_is_one() {
        let mut s = scroll(50, 10);
        s.offset = 40;
        assert!((s.percentage() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn percentage_in_middle() {
        let mut s = scroll(50, 10);
        s.offset = 20;
        assert!((s.percentage() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn percentage_total_zero() {
        let s = scroll(0, 10);
        assert_eq!(s.percentage(), 0.0);
    }

    #[test]
    fn percentage_content_fits_viewport() {
        let s = scroll(5, 10);
        assert_eq!(s.percentage(), 0.0);
    }

    #[test]
    fn percentage_total_equals_visible() {
        let s = scroll(10, 10);
        assert_eq!(s.percentage(), 0.0);
    }

    #[test]
    fn percentage_range_0_to_1() {
        let total = 30;
        let visible = 10;
        let max_offset = total - visible;
        for offset in 0..=max_offset {
            let s = PanelScrollState {
                offset,
                cursor: offset,
                total,
                visible,
            };
            let pct = s.percentage();
            assert!(
                (0.0..=1.0).contains(&pct),
                "percentage {} out of range for offset {}",
                pct,
                offset
            );
        }
    }

    // ── selected / is_selected ───────────────────────────────────────────

    #[test]
    fn selected_returns_none_when_empty() {
        let s = scroll(0, 10);
        assert!(s.selected().is_none());
    }

    #[test]
    fn selected_returns_cursor() {
        let mut s = scroll(10, 5);
        s.cursor = 3;
        assert_eq!(s.selected(), Some(3));
    }

    #[test]
    fn is_selected_checks_cursor() {
        let mut s = scroll(10, 5);
        s.cursor = 7;
        assert!(s.is_selected(7));
        assert!(!s.is_selected(6));
    }

    // ── at_top / at_bottom ───────────────────────────────────────────────

    #[test]
    fn at_top_initially() {
        let s = scroll(50, 10);
        assert!(s.at_top());
        assert!(!s.at_bottom());
    }

    #[test]
    fn at_bottom_when_scrolled_to_end() {
        let mut s = scroll(50, 10);
        s.offset = 40;
        assert!(s.at_bottom());
        assert!(!s.at_top());
    }

    #[test]
    fn at_top_and_bottom_when_content_fits() {
        let s = scroll(5, 10);
        assert!(s.at_top());
        assert!(s.at_bottom());
    }

    // ── set_total ────────────────────────────────────────────────────────

    #[test]
    fn set_total_clamps_cursor() {
        let mut s = scroll(20, 10);
        s.cursor = 15;
        s.set_total(10);
        assert_eq!(s.cursor, 9);
    }

    #[test]
    fn set_total_adjusts_offset() {
        let mut s = scroll(50, 10);
        s.offset = 40;
        s.set_total(20);
        assert_eq!(s.offset, 10);
    }

    #[test]
    fn set_total_to_zero() {
        let mut s = scroll(10, 5);
        s.cursor = 5;
        s.set_total(0);
        assert_eq!(s.total, 0);
    }

    // ── set_visible ──────────────────────────────────────────────────────

    #[test]
    fn set_visible_updates_and_adjusts() {
        let mut s = scroll(50, 10);
        s.cursor = 30;
        s.offset = 0;
        s.set_visible(20);
        assert_eq!(s.visible, 20);
        let range = s.visible_range();
        assert!(s.cursor >= range.start && s.cursor < range.end);
    }

    // ── scroll_down / scroll_up (offset) ─────────────────────────────────

    #[test]
    fn scroll_down_offset_increments() {
        let mut s = scroll(20, 10);
        s.scroll_down();
        assert_eq!(s.offset, 1);
    }

    #[test]
    fn scroll_down_stops_at_max() {
        let mut s = scroll(20, 10);
        s.offset = 10;
        s.scroll_down();
        assert_eq!(s.offset, 10);
    }

    #[test]
    fn scroll_up_offset_decrements() {
        let mut s = scroll(20, 10);
        s.offset = 5;
        s.scroll_up();
        assert_eq!(s.offset, 4);
    }

    #[test]
    fn scroll_up_at_zero_is_noop() {
        let mut s = scroll(20, 10);
        s.scroll_up();
        assert_eq!(s.offset, 0);
    }

    // ── cursor_first / cursor_last ───────────────────────────────────────

    #[test]
    fn cursor_first_moves_to_zero() {
        let mut s = scroll(50, 10);
        s.cursor = 30;
        s.offset = 25;
        s.cursor_first();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn cursor_last_moves_to_end() {
        let mut s = scroll(50, 10);
        s.cursor_last();
        assert_eq!(s.cursor, 49);
    }

    #[test]
    fn cursor_last_with_zero_items_is_noop() {
        let mut s = scroll(0, 10);
        s.cursor_last();
        assert_eq!(s.cursor, 0);
    }

    // ── indicator ────────────────────────────────────────────────────────

    #[test]
    fn indicator_none_when_content_fits() {
        let s = scroll(5, 10);
        assert!(s.indicator().is_none());
    }

    #[test]
    fn indicator_at_top_shows_down_arrow() {
        let s = scroll(50, 10);
        let ind = s.indicator().unwrap();
        assert!(
            ind.contains('\u{2193}'),
            "expected down arrow, got: {}",
            ind
        );
        assert!(ind.contains("Top"), "expected Top, got: {}", ind);
    }

    #[test]
    fn indicator_at_bottom_shows_up_arrow() {
        let mut s = scroll(50, 10);
        s.offset = 40;
        let ind = s.indicator().unwrap();
        assert!(ind.contains('\u{2191}'), "expected up arrow, got: {}", ind);
        assert!(ind.contains("Bot"), "expected Bot, got: {}", ind);
    }

    #[test]
    fn indicator_in_middle_shows_both_arrows() {
        let mut s = scroll(50, 10);
        s.offset = 20;
        let ind = s.indicator().unwrap();
        assert!(
            ind.contains('\u{2195}'),
            "expected both-arrows, got: {}",
            ind
        );
        assert!(ind.contains('%'), "expected percentage, got: {}", ind);
    }

    // ── constructors / constants ─────────────────────────────────────────

    #[test]
    fn with_total_sets_total() {
        let s = PanelScrollState::with_total(42);
        assert_eq!(s.total, 42);
        assert_eq!(s.cursor, 0);
        assert_eq!(s.offset, 0);
        assert_eq!(s.visible, 0);
    }

    #[test]
    fn scroll_margin_is_three() {
        assert_eq!(SCROLL_MARGIN, 3);
    }
}
