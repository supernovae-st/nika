// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Render Mode for TaskBox Widgets
//!
//! Controls detail level: Compact (1-line), Expanded (default), Full (all).

/// Render mode controlling TaskBox detail level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Single line summary
    Compact,
    /// Default view with collapsed sections
    #[default]
    Expanded,
    /// All sections expanded with full metrics
    Full,
}

impl RenderMode {
    /// Base height for this mode (without content)
    pub const fn base_height(&self) -> u16 {
        match self {
            Self::Compact => 1,
            Self::Expanded => 5,
            Self::Full => 10,
        }
    }

    /// Cycle to next mode
    pub const fn cycle(&self) -> Self {
        match self {
            Self::Compact => Self::Expanded,
            Self::Expanded => Self::Full,
            Self::Full => Self::Compact,
        }
    }

    /// Whether to show params section
    pub const fn shows_params(&self) -> bool {
        matches!(self, Self::Expanded | Self::Full)
    }

    /// Whether to show result section
    pub const fn shows_result(&self) -> bool {
        matches!(self, Self::Expanded | Self::Full)
    }

    /// Whether to show detailed metrics
    pub const fn shows_metrics(&self) -> bool {
        matches!(self, Self::Full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_mode_default() {
        let mode = RenderMode::default();
        assert_eq!(mode, RenderMode::Expanded);
    }

    #[test]
    fn test_render_mode_heights() {
        assert_eq!(RenderMode::Compact.base_height(), 1);
        assert_eq!(RenderMode::Expanded.base_height(), 5);
        assert_eq!(RenderMode::Full.base_height(), 10);
    }

    #[test]
    fn test_render_mode_cycle() {
        let mode = RenderMode::Compact;
        assert_eq!(mode.cycle(), RenderMode::Expanded);
        assert_eq!(RenderMode::Expanded.cycle(), RenderMode::Full);
        assert_eq!(RenderMode::Full.cycle(), RenderMode::Compact);
    }

    #[test]
    fn test_render_mode_shows_section() {
        // Compact shows nothing extra
        assert!(!RenderMode::Compact.shows_params());
        assert!(!RenderMode::Compact.shows_result());
        assert!(!RenderMode::Compact.shows_metrics());

        // Expanded shows params and result (collapsed)
        assert!(RenderMode::Expanded.shows_params());
        assert!(RenderMode::Expanded.shows_result());
        assert!(!RenderMode::Expanded.shows_metrics());

        // Full shows everything
        assert!(RenderMode::Full.shows_params());
        assert!(RenderMode::Full.shows_result());
        assert!(RenderMode::Full.shows_metrics());
    }
}
