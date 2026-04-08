// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Solarized color palette constants and animation helpers.
//!
//! The Solarized palette by Ethan Schoonover provides a semantic gray scale
//! that adapts to both light and dark themes. Accent colors are used for
//! animated glow and pulse effects.

/// Solarized base colors
/// Solarized color palette by Ethan Schoonover.
///
/// The base colors (BASE03-BASE3) form a semantic gray scale that adapts
/// to both light and dark themes. Unused constants are preserved for
/// future theme customization.
#[allow(dead_code)]
pub mod solarized {
    use ratatui::style::Color;

    // Base colors (dark theme)
    pub const BASE03: Color = Color::Rgb(0, 43, 54); // #002b36 dark bg
    pub const BASE02: Color = Color::Rgb(7, 54, 66); // #073642 dark bg highlight
    pub const BASE01: Color = Color::Rgb(88, 110, 117); // #586e75 comments
    pub const BASE00: Color = Color::Rgb(101, 123, 131); // #657b83 body text
    pub const BASE0: Color = Color::Rgb(131, 148, 150); // #839496 primary content
    pub const BASE1: Color = Color::Rgb(147, 161, 161); // #93a1a1 emphasized
    pub const BASE2: Color = Color::Rgb(238, 232, 213); // #eee8d5 light bg highlight
    pub const BASE3: Color = Color::Rgb(253, 246, 227); // #fdf6e3 light bg

    // Accent colors
    pub const YELLOW: Color = Color::Rgb(181, 137, 0); // #b58900
    pub const ORANGE: Color = Color::Rgb(203, 75, 22); // #cb4b16
    pub const RED: Color = Color::Rgb(220, 50, 47); // #dc322f
    pub const MAGENTA: Color = Color::Rgb(211, 54, 130); // #d33682
    pub const VIOLET: Color = Color::Rgb(108, 113, 196); // #6c71c4
    pub const BLUE: Color = Color::Rgb(38, 139, 210); // #268bd2
    pub const CYAN: Color = Color::Rgb(42, 161, 152); // #2aa198
    pub const GREEN: Color = Color::Rgb(133, 153, 0); // #859900

    /// Get animated color that cycles through Solarized accents
    /// frame: 0-255 animation counter
    pub fn accent_at(frame: u8) -> Color {
        let idx = (frame / 32) % 8;
        match idx {
            0 => YELLOW,
            1 => ORANGE,
            2 => RED,
            3 => MAGENTA,
            4 => VIOLET,
            5 => BLUE,
            6 => CYAN,
            _ => GREEN,
        }
    }

    /// Get pulse intensity for glow effects (0.0 - 1.0)
    /// Returns a smooth wave based on frame counter
    pub fn pulse_intensity(frame: u8) -> f32 {
        let t = (frame as f32 / 255.0) * std::f32::consts::PI * 2.0;
        (t.sin() + 1.0) / 2.0
    }
}
