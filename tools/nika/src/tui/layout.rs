//! Responsive layout system for TUI
//!
//! Adapts panel layouts based on terminal size.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Layout modes based on terminal width
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    /// Compact layout for narrow terminals (<80 columns)
    Compact,
    /// Standard layout for normal terminals (80-120 columns)
    #[default]
    Standard,
    /// Wide layout for large terminals (>120 columns)
    Wide,
}

impl LayoutMode {
    /// Determine layout mode from terminal width
    pub fn from_width(width: u16) -> Self {
        match width {
            0..=79 => Self::Compact,
            80..=120 => Self::Standard,
            _ => Self::Wide,
        }
    }

    /// Get label for status bar display
    pub fn label(&self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Standard => "standard",
            Self::Wide => "wide",
        }
    }
}

/// Responsive layout calculator
#[derive(Debug, Clone)]
pub struct ResponsiveLayout {
    /// Current layout mode
    mode: LayoutMode,
    /// Terminal dimensions
    area: Rect,
}

impl ResponsiveLayout {
    /// Create new responsive layout for given area
    pub fn new(area: Rect) -> Self {
        Self {
            mode: LayoutMode::from_width(area.width),
            area,
        }
    }

    /// Get current layout mode
    pub fn mode(&self) -> LayoutMode {
        self.mode
    }

    /// Get available area
    pub fn area(&self) -> Rect {
        self.area
    }

    /// Calculate horizontal split with responsive constraints
    pub fn horizontal_split(&self, ratios: &[u16]) -> Vec<Rect> {
        let constraints: Vec<Constraint> =
            ratios.iter().map(|&r| Constraint::Percentage(r)).collect();

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(self.area)
            .to_vec()
    }

    /// Calculate vertical split with responsive constraints
    pub fn vertical_split(&self, ratios: &[u16]) -> Vec<Rect> {
        let constraints: Vec<Constraint> =
            ratios.iter().map(|&r| Constraint::Percentage(r)).collect();

        Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(self.area)
            .to_vec()
    }

    /// Get panel constraints for current mode (Monitor view 4-panel layout)
    pub fn monitor_constraints(&self) -> (Constraint, Constraint, Constraint, Constraint) {
        match self.mode {
            LayoutMode::Compact => (
                Constraint::Percentage(100), // DAG takes full width
                Constraint::Percentage(0),
                Constraint::Percentage(0),
                Constraint::Percentage(0),
            ),
            LayoutMode::Standard => (
                Constraint::Percentage(50),
                Constraint::Percentage(50),
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ),
            LayoutMode::Wide => (
                Constraint::Percentage(40),
                Constraint::Percentage(30),
                Constraint::Percentage(30),
                Constraint::Percentage(30),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_mode_from_width() {
        assert_eq!(LayoutMode::from_width(60), LayoutMode::Compact);
        assert_eq!(LayoutMode::from_width(80), LayoutMode::Standard);
        assert_eq!(LayoutMode::from_width(100), LayoutMode::Standard);
        assert_eq!(LayoutMode::from_width(150), LayoutMode::Wide);
    }

    #[test]
    fn test_layout_mode_labels() {
        assert_eq!(LayoutMode::Compact.label(), "compact");
        assert_eq!(LayoutMode::Standard.label(), "standard");
        assert_eq!(LayoutMode::Wide.label(), "wide");
    }

    #[test]
    fn test_responsive_layout_new() {
        let area = Rect::new(0, 0, 100, 50);
        let layout = ResponsiveLayout::new(area);
        assert_eq!(layout.mode(), LayoutMode::Standard);
        assert_eq!(layout.area(), area);
    }

    #[test]
    fn test_responsive_layout_horizontal_split() {
        let area = Rect::new(0, 0, 100, 50);
        let layout = ResponsiveLayout::new(area);
        let rects = layout.horizontal_split(&[50, 50]);
        assert_eq!(rects.len(), 2);
    }

    #[test]
    fn test_responsive_layout_vertical_split() {
        let area = Rect::new(0, 0, 100, 50);
        let layout = ResponsiveLayout::new(area);
        let rects = layout.vertical_split(&[30, 70]);
        assert_eq!(rects.len(), 2);
    }
}
