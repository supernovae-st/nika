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
    /// v0.8.1: NovaNet pattern - scroll works before render sets `visible`
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
    /// v0.8.1: NovaNet pattern - scroll works before render sets `visible`
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
