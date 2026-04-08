// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Terminal color mode detection and adaptation.
//!
//! Automatically detects terminal color capabilities based on environment
//! variables. Falls back gracefully for limited terminals.
//!
//! ```rust,ignore
//! let mode = ColorMode::detect();
//! let color = mode.adapt_color(Color::Rgb(139, 92, 246));
//! ```

use ratatui::style::Color;

/// Terminal color mode detection.
///
/// Automatically detects terminal color capabilities based on environment
/// variables. Falls back gracefully for limited terminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// 24-bit TrueColor (16 million colors)
    /// Detected via COLORTERM=truecolor or COLORTERM=24bit
    #[default]
    TrueColor,

    /// 256-color palette (8-bit)
    /// Detected via TERM containing "256color"
    Color256,

    /// Basic 16-color palette (4-bit)
    /// Fallback for basic terminals
    Color16,
}

impl ColorMode {
    /// Detect color mode from environment variables.
    ///
    /// Checks in order:
    /// 1. COLORTERM for truecolor/24bit support
    /// 2. TERM for 256color support
    /// 3. Falls back to 16 colors
    pub fn detect() -> Self {
        // Check COLORTERM for modern terminals
        if let Ok(colorterm) = std::env::var("COLORTERM") {
            let ct = colorterm.to_lowercase();
            if ct == "truecolor" || ct == "24bit" {
                return Self::TrueColor;
            }
        }

        // Check TERM for 256 color support
        if let Ok(term) = std::env::var("TERM") {
            if term.contains("256color") || term.contains("24bit") {
                return Self::Color256;
            }
            // Some terminals advertise truecolor via TERM
            if term.contains("truecolor") {
                return Self::TrueColor;
            }
        }

        // Default to 16 colors for maximum compatibility
        Self::Color16
    }

    /// Check if this mode supports RGB colors.
    pub fn supports_rgb(&self) -> bool {
        matches!(self, Self::TrueColor)
    }

    /// Check if this mode supports at least 256 colors.
    pub fn supports_256(&self) -> bool {
        matches!(self, Self::TrueColor | Self::Color256)
    }

    /// Adapt an RGB color for this color mode.
    ///
    /// - TrueColor: Returns the color unchanged
    /// - Color256: Converts to nearest 256-color palette entry
    /// - Color16: Converts to nearest ANSI color
    pub fn adapt_color(&self, color: Color) -> Color {
        match self {
            Self::TrueColor => color,
            Self::Color256 => Self::rgb_to_256(color),
            Self::Color16 => Self::rgb_to_16(color),
        }
    }

    /// Convert RGB color to nearest 256-color palette entry.
    fn rgb_to_256(color: Color) -> Color {
        match color {
            Color::Rgb(r, g, b) => {
                // Use the 6x6x6 color cube (colors 16-231)
                let r_idx = (r as u16 * 5 / 255) as u8;
                let g_idx = (g as u16 * 5 / 255) as u8;
                let b_idx = (b as u16 * 5 / 255) as u8;
                let idx = 16 + 36 * r_idx + 6 * g_idx + b_idx;
                Color::Indexed(idx)
            }
            other => other,
        }
    }

    /// Convert RGB color to nearest ANSI 16-color.
    fn rgb_to_16(color: Color) -> Color {
        match color {
            Color::Rgb(r, g, b) => {
                // Calculate luminance (use u32 to avoid overflow: 255*587 = 149,685 > u16::MAX)
                let luma = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
                let bright = luma > 127;

                // Find dominant color channel
                let max = r.max(g).max(b);
                let min = r.min(g).min(b);
                let saturation = if max == 0 {
                    0
                } else {
                    (max - min) as u16 * 255 / max as u16
                };

                if saturation < 50 {
                    // Grayscale
                    if bright {
                        Color::White
                    } else {
                        Color::DarkGray
                    }
                } else if r >= g && r >= b {
                    // Red dominant - check if green is significant enough for yellow
                    // Yellow needs both red AND green to be high (> 100), not just r > b
                    if g > 100 && g > r / 2 {
                        if bright {
                            Color::Yellow
                        } else {
                            Color::LightYellow
                        }
                    } else if bright {
                        Color::LightRed
                    } else {
                        Color::Red
                    }
                } else if g >= r && g >= b {
                    // Green dominant - check if blue is significant enough for cyan
                    // Cyan needs both green AND blue to be high (> 100)
                    if b > 100 && b > g / 2 {
                        if bright {
                            Color::Cyan
                        } else {
                            Color::LightCyan
                        }
                    } else if bright {
                        Color::LightGreen
                    } else {
                        Color::Green
                    }
                } else {
                    // Blue dominant - check if red is significant enough for magenta
                    // Magenta needs both blue AND red to be high (> 100)
                    if r > 100 && r > b / 2 {
                        if bright {
                            Color::LightMagenta
                        } else {
                            Color::Magenta
                        }
                    } else if bright {
                        Color::LightBlue
                    } else {
                        Color::Blue
                    }
                }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_mode_default_is_truecolor() {
        let mode = ColorMode::default();
        assert_eq!(mode, ColorMode::TrueColor);
    }

    #[test]
    fn test_color_mode_supports_rgb_truecolor() {
        assert!(ColorMode::TrueColor.supports_rgb());
        assert!(!ColorMode::Color256.supports_rgb());
        assert!(!ColorMode::Color16.supports_rgb());
    }

    #[test]
    fn test_color_mode_supports_256() {
        assert!(ColorMode::TrueColor.supports_256());
        assert!(ColorMode::Color256.supports_256());
        assert!(!ColorMode::Color16.supports_256());
    }

    #[test]
    fn test_color_mode_adapt_color_truecolor_unchanged() {
        let color = Color::Rgb(139, 92, 246);
        assert_eq!(ColorMode::TrueColor.adapt_color(color), color);
    }

    #[test]
    fn test_color_mode_adapt_color_256_converts_to_indexed() {
        let color = Color::Rgb(139, 92, 246);
        let adapted = ColorMode::Color256.adapt_color(color);

        // Should convert to indexed (8-bit) color
        match adapted {
            Color::Indexed(_) => (), // Expected
            _ => panic!("Expected Indexed color, got {:?}", adapted),
        }
    }

    #[test]
    fn test_color_mode_adapt_color_16_converts_to_ansi() {
        let color = Color::Rgb(139, 92, 246);
        let adapted = ColorMode::Color16.adapt_color(color);

        // Should convert to ANSI color (not RGB)
        if let Color::Rgb(_, _, _) = adapted {
            panic!("Should not be RGB for Color16 mode")
        }
        // Expected: White, DarkGray, or other ANSI color
    }

    #[test]
    fn test_color_mode_256_adapt_converts_to_indexed() {
        // Test conversion to 6x6x6 color cube (colors 16-231)
        let mode = ColorMode::Color256;
        let red = mode.adapt_color(Color::Rgb(255, 0, 0));
        let green = mode.adapt_color(Color::Rgb(0, 255, 0));
        let blue = mode.adapt_color(Color::Rgb(0, 0, 255));

        // All should be Indexed colors in range 16-231
        match (red, green, blue) {
            (Color::Indexed(r), Color::Indexed(g), Color::Indexed(b)) => {
                assert!((16..=231).contains(&r));
                assert!((16..=231).contains(&g));
                assert!((16..=231).contains(&b));
            }
            _ => panic!("Expected Indexed colors"),
        }
    }

    #[test]
    fn test_color_mode_256_adapt_preserves_non_rgb() {
        // Non-RGB colors should pass through unchanged
        let mode = ColorMode::Color256;
        let indexed = mode.adapt_color(Color::Indexed(100));
        assert_eq!(indexed, Color::Indexed(100));

        let white = mode.adapt_color(Color::White);
        assert_eq!(white, Color::White);
    }

    #[test]
    fn test_color_mode_16_adapt_grayscale() {
        // Grayscale should become White or DarkGray
        let mode = ColorMode::Color16;
        let light_gray = mode.adapt_color(Color::Rgb(200, 200, 200));
        assert_eq!(light_gray, Color::White);

        let dark_gray = mode.adapt_color(Color::Rgb(50, 50, 50));
        assert_eq!(dark_gray, Color::DarkGray);
    }

    #[test]
    fn test_color_mode_16_adapt_red_dominant() {
        // Red dominant with enough luma for brightness (luma > 127)
        // (255, 80, 60) -> luma ~130, saturation ~195, red dominant, g < 100 -> LightRed
        let mode = ColorMode::Color16;
        let red = mode.adapt_color(Color::Rgb(255, 80, 60));
        assert_eq!(red, Color::LightRed);
    }

    #[test]
    fn test_color_mode_16_adapt_green_dominant() {
        // High green dominant - green has highest luma coefficient
        let mode = ColorMode::Color16;
        let green = mode.adapt_color(Color::Rgb(30, 200, 30));
        assert_eq!(green, Color::LightGreen);
    }

    #[test]
    fn test_color_mode_16_adapt_blue_dominant() {
        // Blue dominant with enough luma for brightness
        // (60, 150, 200) -> luma ~129, blue dominant, r < 100 -> LightBlue
        let mode = ColorMode::Color16;
        let blue = mode.adapt_color(Color::Rgb(60, 150, 200));
        assert_eq!(blue, Color::LightBlue);
    }

    #[test]
    fn test_color_mode_16_adapt_preserves_non_rgb() {
        let mode = ColorMode::Color16;
        let white = mode.adapt_color(Color::White);
        assert_eq!(white, Color::White);

        let indexed = mode.adapt_color(Color::Indexed(50));
        assert_eq!(indexed, Color::Indexed(50));
    }

    #[test]
    fn test_color_mode_detect_consistency() {
        // Multiple calls to detect should return the same result
        // (unless environment variables change)
        let mode1 = ColorMode::detect();
        let mode2 = ColorMode::detect();
        assert_eq!(mode1, mode2);
    }
}
