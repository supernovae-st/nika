//! Scroll Indicator Widget
//!
//! Displays a vertical scroll bar to indicate scroll position.
//! Uses Unicode block characters for smooth appearance.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::fmt;

/// Scroll indicator widget - displays a vertical scrollbar
///
/// Shows current scroll position within a scrollable area using
/// Unicode block characters for a smooth appearance.
///
/// # Example
///
/// ```ignore
/// let scroll = ScrollIndicator::new()
///     .position(current_offset, total_items, visible_height)
///     .style(Style::default().fg(Color::DarkGray));
/// ```
pub struct ScrollIndicator {
    /// Current scroll offset (first visible item)
    offset: usize,
    /// Total number of items
    total: usize,
    /// Number of visible items
    visible: usize,
    /// Track style (background)
    track_style: Style,
    /// Thumb style (the draggable part)
    thumb_style: Style,
    /// Show arrows at top/bottom when scrollable
    show_arrows: bool,
}

impl Default for ScrollIndicator {
    fn default() -> Self {
        Self {
            offset: 0,
            total: 0,
            visible: 0,
            track_style: Style::default().fg(Color::DarkGray),
            thumb_style: Style::default().fg(Color::Cyan),
            show_arrows: true,
        }
    }
}

impl ScrollIndicator {
    /// Create a new scroll indicator
    pub fn new() -> Self {
        Self::default()
    }

    /// Set scroll position
    ///
    /// - `offset`: Current scroll offset (first visible item index)
    /// - `total`: Total number of items
    /// - `visible`: Number of items that fit in the visible area
    pub fn position(mut self, offset: usize, total: usize, visible: usize) -> Self {
        self.offset = offset;
        self.total = total;
        self.visible = visible;
        self
    }

    /// Set track style (the background line)
    pub fn track_style(mut self, style: Style) -> Self {
        self.track_style = style;
        self
    }

    /// Set thumb style (the draggable indicator)
    pub fn thumb_style(mut self, style: Style) -> Self {
        self.thumb_style = style;
        self
    }

    /// Enable/disable arrows at ends
    pub fn show_arrows(mut self, show: bool) -> Self {
        self.show_arrows = show;
        self
    }

    /// Returns true if scrolling is possible (more items than visible)
    pub fn is_scrollable(&self) -> bool {
        self.total > self.visible
    }

    /// Returns true if can scroll up
    pub fn can_scroll_up(&self) -> bool {
        self.offset > 0
    }

    /// Returns true if can scroll down
    pub fn can_scroll_down(&self) -> bool {
        self.offset + self.visible < self.total
    }

    /// Calculate thumb position and size
    fn calculate_thumb(&self, track_height: usize) -> (usize, usize) {
        if !self.is_scrollable() || track_height == 0 {
            return (0, track_height);
        }

        // Thumb size is proportional to visible/total ratio
        // Minimum thumb size is 1
        let thumb_size = ((self.visible as f64 / self.total as f64) * track_height as f64)
            .max(1.0)
            .min(track_height as f64) as usize;

        // Thumb position
        let max_offset = self.total.saturating_sub(self.visible);
        let scrollable_track = track_height.saturating_sub(thumb_size);

        let thumb_pos = if max_offset > 0 {
            ((self.offset as f64 / max_offset as f64) * scrollable_track as f64) as usize
        } else {
            0
        };

        (thumb_pos, thumb_size)
    }
}

impl Widget for ScrollIndicator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let height = area.height as usize;

        // If not scrollable, just draw a faint track
        if !self.is_scrollable() {
            for y in 0..height {
                let cell = buf.cell_mut((area.x, area.y + y as u16)).unwrap();
                cell.set_char('│');
                cell.set_style(self.track_style);
            }
            return;
        }

        // Calculate track height (reserve space for arrows if shown)
        let (track_start, track_height) = if self.show_arrows && height >= 3 {
            (1, height - 2)
        } else {
            (0, height)
        };

        // Draw arrows if enabled
        if self.show_arrows && height >= 3 {
            // Up arrow
            let up_cell = buf.cell_mut((area.x, area.y)).unwrap();
            if self.can_scroll_up() {
                up_cell.set_char('▲');
                up_cell.set_style(self.thumb_style);
            } else {
                up_cell.set_char('△');
                up_cell.set_style(self.track_style);
            }

            // Down arrow
            let down_cell = buf.cell_mut((area.x, area.y + area.height - 1)).unwrap();
            if self.can_scroll_down() {
                down_cell.set_char('▼');
                down_cell.set_style(self.thumb_style);
            } else {
                down_cell.set_char('▽');
                down_cell.set_style(self.track_style);
            }
        }

        // Calculate thumb
        let (thumb_pos, thumb_size) = self.calculate_thumb(track_height);

        // Draw track and thumb
        for i in 0..track_height {
            let y = area.y + (track_start + i) as u16;
            let cell = buf.cell_mut((area.x, y)).unwrap();

            if i >= thumb_pos && i < thumb_pos + thumb_size {
                // Thumb
                cell.set_char('█');
                cell.set_style(self.thumb_style);
            } else {
                // Track
                cell.set_char('░');
                cell.set_style(self.track_style);
            }
        }
    }
}

/// Compact scroll hint for inline display
///
/// Shows a brief indicator like "↑3↓" or "▼5" to indicate scroll state
pub struct ScrollHint {
    /// Items above viewport
    above: usize,
    /// Items below viewport
    below: usize,
    /// Style for the hint
    style: Style,
}

impl ScrollHint {
    /// Create a new scroll hint
    pub fn new(above: usize, below: usize) -> Self {
        Self {
            above,
            below,
            style: Style::default().fg(Color::DarkGray),
        }
    }

    /// Set hint style
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl fmt::Display for ScrollHint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.above > 0, self.below > 0) {
            (true, true) => write!(f, "↑{}↓{}", self.above, self.below),
            (true, false) => write!(f, "↑{}", self.above),
            (false, true) => write!(f, "↓{}", self.below),
            (false, false) => Ok(()),
        }
    }
}

impl Widget for ScrollHint {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 {
            return;
        }

        let text = self.to_string();
        if text.is_empty() {
            return;
        }

        // Render at right side of area
        let text_len = text.chars().count() as u16;
        let start_x = if area.width >= text_len {
            area.x + area.width - text_len
        } else {
            area.x
        };

        for (i, ch) in text.chars().enumerate() {
            let x = start_x + i as u16;
            if x < area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, area.y)) {
                    cell.set_char(ch);
                    cell.set_style(self.style);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_indicator_default() {
        let indicator = ScrollIndicator::new();
        assert!(!indicator.is_scrollable());
        assert!(!indicator.can_scroll_up());
        assert!(!indicator.can_scroll_down());
    }

    #[test]
    fn test_scroll_indicator_scrollable() {
        let indicator = ScrollIndicator::new().position(0, 100, 20);
        assert!(indicator.is_scrollable());
        assert!(!indicator.can_scroll_up());
        assert!(indicator.can_scroll_down());
    }

    #[test]
    fn test_scroll_indicator_middle() {
        let indicator = ScrollIndicator::new().position(40, 100, 20);
        assert!(indicator.is_scrollable());
        assert!(indicator.can_scroll_up());
        assert!(indicator.can_scroll_down());
    }

    #[test]
    fn test_scroll_indicator_at_bottom() {
        let indicator = ScrollIndicator::new().position(80, 100, 20);
        assert!(indicator.is_scrollable());
        assert!(indicator.can_scroll_up());
        assert!(!indicator.can_scroll_down());
    }

    #[test]
    fn test_thumb_calculation() {
        let indicator = ScrollIndicator::new().position(0, 100, 20);
        let (pos, size) = indicator.calculate_thumb(20);
        // Thumb should be at top (pos=0) and relatively small
        assert_eq!(pos, 0);
        assert!(size > 0 && size < 20);
    }

    #[test]
    fn test_scroll_hint_formatting() {
        assert_eq!(ScrollHint::new(3, 7).to_string(), "↑3↓7");
        assert_eq!(ScrollHint::new(5, 0).to_string(), "↑5");
        assert_eq!(ScrollHint::new(0, 10).to_string(), "↓10");
        assert_eq!(ScrollHint::new(0, 0).to_string(), "");
    }

    #[test]
    fn test_not_scrollable_when_all_visible() {
        let indicator = ScrollIndicator::new().position(0, 10, 20);
        assert!(!indicator.is_scrollable());
    }
}
