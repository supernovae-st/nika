// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Highlight Theme Module
//!
//! Defines color mappings for syntax highlighting captures.
//! Uses Solarized palette for consistency with Nika's TUI theme.

use ratatui::style::{Color, Modifier, Style};

use super::HighlightCapture;

/// Solarized color palette
mod colors {
    use ratatui::style::Color;

    // Base colors (common)
    pub const BASE01: Color = Color::Rgb(88, 110, 117); // Comments, secondary content
    pub const BASE00: Color = Color::Rgb(101, 123, 131); // Body text (dark)
    pub const BASE0: Color = Color::Rgb(131, 148, 150); // Body text (light)

    // Accent colors
    pub const YELLOW: Color = Color::Rgb(181, 137, 0);
    pub const ORANGE: Color = Color::Rgb(203, 75, 22);
    pub const RED: Color = Color::Rgb(220, 50, 47);
    pub const MAGENTA: Color = Color::Rgb(211, 54, 130);
    pub const VIOLET: Color = Color::Rgb(108, 113, 196);
    pub const BLUE: Color = Color::Rgb(38, 139, 210);
    pub const CYAN: Color = Color::Rgb(42, 161, 152);
    pub const GREEN: Color = Color::Rgb(133, 153, 0);
}

/// Trait for highlight themes
pub trait HighlightTheme {
    /// Get the style for a highlight capture
    fn style(&self, capture: HighlightCapture) -> Style;
}

/// Solarized-based syntax highlighting theme
///
/// Colors are designed to work on both light and dark backgrounds.
/// Uses semantic colors that match Nika's TUI Solarized theme.
#[derive(Debug, Clone, Copy, Default)]
pub struct SolarizedTheme {
    /// Whether to use dark mode colors
    pub dark_mode: bool,
}

impl SolarizedTheme {
    /// Create a new Solarized theme
    pub fn new(dark_mode: bool) -> Self {
        Self { dark_mode }
    }
}

impl HighlightTheme for SolarizedTheme {
    fn style(&self, capture: HighlightCapture) -> Style {
        use colors::*;

        let fg = match capture {
            // YAML structure
            HighlightCapture::Key => BLUE,
            HighlightCapture::String => CYAN,
            HighlightCapture::Number => MAGENTA,
            HighlightCapture::Boolean => ORANGE,
            HighlightCapture::Null => ORANGE,
            HighlightCapture::Comment => BASE01,
            HighlightCapture::Punctuation => {
                if self.dark_mode {
                    BASE0
                } else {
                    BASE00
                }
            }

            // Nika-specific
            HighlightCapture::Verb => GREEN,
            HighlightCapture::TaskId => YELLOW,
            HighlightCapture::Template => VIOLET,
            HighlightCapture::McpServer => CYAN,

            // Errors
            HighlightCapture::Error => RED,
        };

        let mut style = Style::default().fg(fg);

        // Add modifiers for certain captures
        match capture {
            HighlightCapture::Key => {
                style = style.add_modifier(Modifier::BOLD);
            }
            HighlightCapture::Verb => {
                style = style.add_modifier(Modifier::BOLD);
            }
            HighlightCapture::TaskId => {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            HighlightCapture::Comment => {
                style = style.add_modifier(Modifier::ITALIC);
            }
            HighlightCapture::Error => {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            _ => {}
        }

        style
    }
}

// Suppress unused warning for Color import (used in module)
const _: Color = Color::Reset;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solarized_theme_dark() {
        let theme = SolarizedTheme::new(true);

        let key_style = theme.style(HighlightCapture::Key);
        assert!(key_style.fg.is_some());
        assert!(key_style.add_modifier.contains(Modifier::BOLD));

        let comment_style = theme.style(HighlightCapture::Comment);
        assert!(comment_style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_solarized_theme_light() {
        let theme = SolarizedTheme::new(false);

        let punct_style = theme.style(HighlightCapture::Punctuation);
        assert!(punct_style.fg.is_some());
    }

    #[test]
    fn test_nika_specific_captures() {
        let theme = SolarizedTheme::new(true);

        let verb_style = theme.style(HighlightCapture::Verb);
        assert!(verb_style.add_modifier.contains(Modifier::BOLD));

        let task_id_style = theme.style(HighlightCapture::TaskId);
        assert!(task_id_style.add_modifier.contains(Modifier::UNDERLINED));
    }
}
