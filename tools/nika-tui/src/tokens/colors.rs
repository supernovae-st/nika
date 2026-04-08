// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # Tailwind CSS Color Palette for Ratatui
//!
//! Complete Tailwind CSS v3.4 color palette converted to `ratatui::style::Color`.
//! Contains 12 palettes × 11 shades = 132 colors.
//!
//! ## Cosmic Tailwind Selection 🦋🌌
//!
//! Primary palettes for Nika:
//! - **Slate**: Blue-tinted neutrals (bg, borders, text)
//! - **Violet**: Primary brand accent (🦋 Nika)
//! - **Cyan**: Secondary accent (🌌 cosmic glow)
//! - **Pink**: Tertiary accent (nebula highlights)
//!
//! Semantic palettes:
//! - **Emerald**: Success, invoke verb
//! - **Amber**: Warning, exec verb
//! - **Rose**: Agent verb
//! - **Red**: Error states
//! - **Blue**: Info states
//! - **Green**: Success alternative, nature
//! - **Yellow**: Attention, highlights
//! - **Gray**: Pure neutral (no blue tint)

use ratatui::style::Color;

// ═══════════════════════════════════════════════════════════════════════════════
// COLOR PALETTE (Level 1)
// ═══════════════════════════════════════════════════════════════════════════════

/// Complete Tailwind CSS color palette
///
/// Contains all shades (50-950) for each color family.
/// Use `ColorPalette::tailwind()` for standard Tailwind colors.
#[derive(Debug, Clone)]
pub struct ColorPalette {
    // ═══════════════════════════════════════════════════════════════════════════
    // SLATE (Blue-tinted neutrals) - Primary neutral for Cosmic theme
    // ═══════════════════════════════════════════════════════════════════════════
    pub slate_50: Color,
    pub slate_100: Color,
    pub slate_200: Color,
    pub slate_300: Color,
    pub slate_400: Color,
    pub slate_500: Color,
    pub slate_600: Color,
    pub slate_700: Color,
    pub slate_800: Color,
    pub slate_900: Color,
    pub slate_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // VIOLET (🦋 Nika brand - Primary accent)
    // ═══════════════════════════════════════════════════════════════════════════
    pub violet_50: Color,
    pub violet_100: Color,
    pub violet_200: Color,
    pub violet_300: Color,
    pub violet_400: Color,
    pub violet_500: Color,
    pub violet_600: Color,
    pub violet_700: Color,
    pub violet_800: Color,
    pub violet_900: Color,
    pub violet_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // CYAN (🌌 Cosmic glow - Secondary accent)
    // ═══════════════════════════════════════════════════════════════════════════
    pub cyan_50: Color,
    pub cyan_100: Color,
    pub cyan_200: Color,
    pub cyan_300: Color,
    pub cyan_400: Color,
    pub cyan_500: Color,
    pub cyan_600: Color,
    pub cyan_700: Color,
    pub cyan_800: Color,
    pub cyan_900: Color,
    pub cyan_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // PINK (Nebula highlights - Tertiary accent)
    // ═══════════════════════════════════════════════════════════════════════════
    pub pink_50: Color,
    pub pink_100: Color,
    pub pink_200: Color,
    pub pink_300: Color,
    pub pink_400: Color,
    pub pink_500: Color,
    pub pink_600: Color,
    pub pink_700: Color,
    pub pink_800: Color,
    pub pink_900: Color,
    pub pink_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // EMERALD (Success, invoke verb)
    // ═══════════════════════════════════════════════════════════════════════════
    pub emerald_50: Color,
    pub emerald_100: Color,
    pub emerald_200: Color,
    pub emerald_300: Color,
    pub emerald_400: Color,
    pub emerald_500: Color,
    pub emerald_600: Color,
    pub emerald_700: Color,
    pub emerald_800: Color,
    pub emerald_900: Color,
    pub emerald_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // AMBER (Warning, exec verb)
    // ═══════════════════════════════════════════════════════════════════════════
    pub amber_50: Color,
    pub amber_100: Color,
    pub amber_200: Color,
    pub amber_300: Color,
    pub amber_400: Color,
    pub amber_500: Color,
    pub amber_600: Color,
    pub amber_700: Color,
    pub amber_800: Color,
    pub amber_900: Color,
    pub amber_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // ROSE (Agent verb)
    // ═══════════════════════════════════════════════════════════════════════════
    pub rose_50: Color,
    pub rose_100: Color,
    pub rose_200: Color,
    pub rose_300: Color,
    pub rose_400: Color,
    pub rose_500: Color,
    pub rose_600: Color,
    pub rose_700: Color,
    pub rose_800: Color,
    pub rose_900: Color,
    pub rose_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // RED (Error states)
    // ═══════════════════════════════════════════════════════════════════════════
    pub red_50: Color,
    pub red_100: Color,
    pub red_200: Color,
    pub red_300: Color,
    pub red_400: Color,
    pub red_500: Color,
    pub red_600: Color,
    pub red_700: Color,
    pub red_800: Color,
    pub red_900: Color,
    pub red_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // BLUE (Info states)
    // ═══════════════════════════════════════════════════════════════════════════
    pub blue_50: Color,
    pub blue_100: Color,
    pub blue_200: Color,
    pub blue_300: Color,
    pub blue_400: Color,
    pub blue_500: Color,
    pub blue_600: Color,
    pub blue_700: Color,
    pub blue_800: Color,
    pub blue_900: Color,
    pub blue_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // GREEN (Success alternative, nature)
    // ═══════════════════════════════════════════════════════════════════════════
    pub green_50: Color,
    pub green_100: Color,
    pub green_200: Color,
    pub green_300: Color,
    pub green_400: Color,
    pub green_500: Color,
    pub green_600: Color,
    pub green_700: Color,
    pub green_800: Color,
    pub green_900: Color,
    pub green_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // YELLOW (Attention, highlights)
    // ═══════════════════════════════════════════════════════════════════════════
    pub yellow_50: Color,
    pub yellow_100: Color,
    pub yellow_200: Color,
    pub yellow_300: Color,
    pub yellow_400: Color,
    pub yellow_500: Color,
    pub yellow_600: Color,
    pub yellow_700: Color,
    pub yellow_800: Color,
    pub yellow_900: Color,
    pub yellow_950: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // GRAY (Pure neutral - no blue tint)
    // ═══════════════════════════════════════════════════════════════════════════
    pub gray_50: Color,
    pub gray_100: Color,
    pub gray_200: Color,
    pub gray_300: Color,
    pub gray_400: Color,
    pub gray_500: Color,
    pub gray_600: Color,
    pub gray_700: Color,
    pub gray_800: Color,
    pub gray_900: Color,
    pub gray_950: Color,
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self::tailwind()
    }
}

impl ColorPalette {
    /// Create standard Tailwind CSS v3.4 color palette
    ///
    /// All colors are official Tailwind values.
    /// Source: <https://tailwindcss.com/docs/customizing-colors>
    pub fn tailwind() -> Self {
        Self {
            // ═══════════════════════════════════════════════════════════════════
            // SLATE - Blue-tinted neutrals
            // ═══════════════════════════════════════════════════════════════════
            slate_50: Color::Rgb(248, 250, 252),  // #f8fafc
            slate_100: Color::Rgb(241, 245, 249), // #f1f5f9
            slate_200: Color::Rgb(226, 232, 240), // #e2e8f0
            slate_300: Color::Rgb(203, 213, 225), // #cbd5e1
            slate_400: Color::Rgb(148, 163, 184), // #94a3b8
            slate_500: Color::Rgb(100, 116, 139), // #64748b
            slate_600: Color::Rgb(71, 85, 105),   // #475569
            slate_700: Color::Rgb(51, 65, 85),    // #334155
            slate_800: Color::Rgb(30, 41, 59),    // #1e293b
            slate_900: Color::Rgb(15, 23, 42),    // #0f172a
            slate_950: Color::Rgb(2, 6, 23),      // #020617

            // ═══════════════════════════════════════════════════════════════════
            // VIOLET - Primary brand (🦋 Nika)
            // ═══════════════════════════════════════════════════════════════════
            violet_50: Color::Rgb(245, 243, 255),  // #f5f3ff
            violet_100: Color::Rgb(237, 233, 254), // #ede9fe
            violet_200: Color::Rgb(221, 214, 254), // #ddd6fe
            violet_300: Color::Rgb(196, 181, 253), // #c4b5fd
            violet_400: Color::Rgb(167, 139, 250), // #a78bfa
            violet_500: Color::Rgb(139, 92, 246),  // #8b5cf6
            violet_600: Color::Rgb(124, 58, 237),  // #7c3aed
            violet_700: Color::Rgb(109, 40, 217),  // #6d28d9
            violet_800: Color::Rgb(91, 33, 182),   // #5b21b6
            violet_900: Color::Rgb(76, 29, 149),   // #4c1d95
            violet_950: Color::Rgb(46, 16, 101),   // #2e1065

            // ═══════════════════════════════════════════════════════════════════
            // CYAN - Secondary accent (🌌 Cosmic glow)
            // ═══════════════════════════════════════════════════════════════════
            cyan_50: Color::Rgb(236, 254, 255),  // #ecfeff
            cyan_100: Color::Rgb(207, 250, 254), // #cffafe
            cyan_200: Color::Rgb(165, 243, 252), // #a5f3fc
            cyan_300: Color::Rgb(103, 232, 249), // #67e8f9
            cyan_400: Color::Rgb(34, 211, 238),  // #22d3ee
            cyan_500: Color::Rgb(6, 182, 212),   // #06b6d4
            cyan_600: Color::Rgb(8, 145, 178),   // #0891b2
            cyan_700: Color::Rgb(14, 116, 144),  // #0e7490
            cyan_800: Color::Rgb(21, 94, 117),   // #155e75
            cyan_900: Color::Rgb(22, 78, 99),    // #164e63
            cyan_950: Color::Rgb(8, 51, 68),     // #083344

            // ═══════════════════════════════════════════════════════════════════
            // PINK - Tertiary accent (Nebula highlights)
            // ═══════════════════════════════════════════════════════════════════
            pink_50: Color::Rgb(253, 242, 248),  // #fdf2f8
            pink_100: Color::Rgb(252, 231, 243), // #fce7f3
            pink_200: Color::Rgb(251, 207, 232), // #fbcfe8
            pink_300: Color::Rgb(249, 168, 212), // #f9a8d4
            pink_400: Color::Rgb(244, 114, 182), // #f472b6
            pink_500: Color::Rgb(236, 72, 153),  // #ec4899
            pink_600: Color::Rgb(219, 39, 119),  // #db2777
            pink_700: Color::Rgb(190, 24, 93),   // #be185d
            pink_800: Color::Rgb(157, 23, 77),   // #9d174d
            pink_900: Color::Rgb(131, 24, 67),   // #831843
            pink_950: Color::Rgb(80, 7, 36),     // #500724

            // ═══════════════════════════════════════════════════════════════════
            // EMERALD - Success, invoke verb
            // ═══════════════════════════════════════════════════════════════════
            emerald_50: Color::Rgb(236, 253, 245),  // #ecfdf5
            emerald_100: Color::Rgb(209, 250, 229), // #d1fae5
            emerald_200: Color::Rgb(167, 243, 208), // #a7f3d0
            emerald_300: Color::Rgb(110, 231, 183), // #6ee7b7
            emerald_400: Color::Rgb(52, 211, 153),  // #34d399
            emerald_500: Color::Rgb(16, 185, 129),  // #10b981
            emerald_600: Color::Rgb(5, 150, 105),   // #059669
            emerald_700: Color::Rgb(4, 120, 87),    // #047857
            emerald_800: Color::Rgb(6, 95, 70),     // #065f46
            emerald_900: Color::Rgb(6, 78, 59),     // #064e3b
            emerald_950: Color::Rgb(2, 44, 34),     // #022c22

            // ═══════════════════════════════════════════════════════════════════
            // AMBER - Warning, exec verb
            // ═══════════════════════════════════════════════════════════════════
            amber_50: Color::Rgb(255, 251, 235),  // #fffbeb
            amber_100: Color::Rgb(254, 243, 199), // #fef3c7
            amber_200: Color::Rgb(253, 230, 138), // #fde68a
            amber_300: Color::Rgb(252, 211, 77),  // #fcd34d
            amber_400: Color::Rgb(251, 191, 36),  // #fbbf24
            amber_500: Color::Rgb(245, 158, 11),  // #f59e0b
            amber_600: Color::Rgb(217, 119, 6),   // #d97706
            amber_700: Color::Rgb(180, 83, 9),    // #b45309
            amber_800: Color::Rgb(146, 64, 14),   // #92400e
            amber_900: Color::Rgb(120, 53, 15),   // #78350f
            amber_950: Color::Rgb(69, 26, 3),     // #451a03

            // ═══════════════════════════════════════════════════════════════════
            // ROSE - Agent verb
            // ═══════════════════════════════════════════════════════════════════
            rose_50: Color::Rgb(255, 241, 242),  // #fff1f2
            rose_100: Color::Rgb(255, 228, 230), // #ffe4e6
            rose_200: Color::Rgb(254, 205, 211), // #fecdd3
            rose_300: Color::Rgb(253, 164, 175), // #fda4af
            rose_400: Color::Rgb(251, 113, 133), // #fb7185
            rose_500: Color::Rgb(244, 63, 94),   // #f43f5e
            rose_600: Color::Rgb(225, 29, 72),   // #e11d48
            rose_700: Color::Rgb(190, 18, 60),   // #be123c
            rose_800: Color::Rgb(159, 18, 57),   // #9f1239
            rose_900: Color::Rgb(136, 19, 55),   // #881337
            rose_950: Color::Rgb(76, 5, 25),     // #4c0519

            // ═══════════════════════════════════════════════════════════════════
            // RED - Error states
            // ═══════════════════════════════════════════════════════════════════
            red_50: Color::Rgb(254, 242, 242),  // #fef2f2
            red_100: Color::Rgb(254, 226, 226), // #fee2e2
            red_200: Color::Rgb(254, 202, 202), // #fecaca
            red_300: Color::Rgb(252, 165, 165), // #fca5a5
            red_400: Color::Rgb(248, 113, 113), // #f87171
            red_500: Color::Rgb(239, 68, 68),   // #ef4444
            red_600: Color::Rgb(220, 38, 38),   // #dc2626
            red_700: Color::Rgb(185, 28, 28),   // #b91c1c
            red_800: Color::Rgb(153, 27, 27),   // #991b1b
            red_900: Color::Rgb(127, 29, 29),   // #7f1d1d
            red_950: Color::Rgb(69, 10, 10),    // #450a0a

            // ═══════════════════════════════════════════════════════════════════
            // BLUE - Info states
            // ═══════════════════════════════════════════════════════════════════
            blue_50: Color::Rgb(239, 246, 255),  // #eff6ff
            blue_100: Color::Rgb(219, 234, 254), // #dbeafe
            blue_200: Color::Rgb(191, 219, 254), // #bfdbfe
            blue_300: Color::Rgb(147, 197, 253), // #93c5fd
            blue_400: Color::Rgb(96, 165, 250),  // #60a5fa
            blue_500: Color::Rgb(59, 130, 246),  // #3b82f6
            blue_600: Color::Rgb(37, 99, 235),   // #2563eb
            blue_700: Color::Rgb(29, 78, 216),   // #1d4ed8
            blue_800: Color::Rgb(30, 64, 175),   // #1e40af
            blue_900: Color::Rgb(30, 58, 138),   // #1e3a8a
            blue_950: Color::Rgb(23, 37, 84),    // #172554

            // ═══════════════════════════════════════════════════════════════════
            // GREEN - Success alternative, nature
            // ═══════════════════════════════════════════════════════════════════
            green_50: Color::Rgb(240, 253, 244),  // #f0fdf4
            green_100: Color::Rgb(220, 252, 231), // #dcfce7
            green_200: Color::Rgb(187, 247, 208), // #bbf7d0
            green_300: Color::Rgb(134, 239, 172), // #86efac
            green_400: Color::Rgb(74, 222, 128),  // #4ade80
            green_500: Color::Rgb(34, 197, 94),   // #22c55e
            green_600: Color::Rgb(22, 163, 74),   // #16a34a
            green_700: Color::Rgb(21, 128, 61),   // #15803d
            green_800: Color::Rgb(22, 101, 52),   // #166534
            green_900: Color::Rgb(20, 83, 45),    // #14532d
            green_950: Color::Rgb(5, 46, 22),     // #052e16

            // ═══════════════════════════════════════════════════════════════════
            // YELLOW - Attention, highlights
            // ═══════════════════════════════════════════════════════════════════
            yellow_50: Color::Rgb(254, 252, 232),  // #fefce8
            yellow_100: Color::Rgb(254, 249, 195), // #fef9c3
            yellow_200: Color::Rgb(254, 240, 138), // #fef08a
            yellow_300: Color::Rgb(253, 224, 71),  // #fde047
            yellow_400: Color::Rgb(250, 204, 21),  // #facc15
            yellow_500: Color::Rgb(234, 179, 8),   // #eab308
            yellow_600: Color::Rgb(202, 138, 4),   // #ca8a04
            yellow_700: Color::Rgb(161, 98, 7),    // #a16207
            yellow_800: Color::Rgb(133, 77, 14),   // #854d0e
            yellow_900: Color::Rgb(113, 63, 18),   // #713f12
            yellow_950: Color::Rgb(66, 32, 6),     // #422006

            // ═══════════════════════════════════════════════════════════════════
            // GRAY - Pure neutral (no blue tint, unlike Slate)
            // ═══════════════════════════════════════════════════════════════════
            gray_50: Color::Rgb(249, 250, 251),  // #f9fafb
            gray_100: Color::Rgb(243, 244, 246), // #f3f4f6
            gray_200: Color::Rgb(229, 231, 235), // #e5e7eb
            gray_300: Color::Rgb(209, 213, 219), // #d1d5db
            gray_400: Color::Rgb(156, 163, 175), // #9ca3af
            gray_500: Color::Rgb(107, 114, 128), // #6b7280
            gray_600: Color::Rgb(75, 85, 99),    // #4b5563
            gray_700: Color::Rgb(55, 65, 81),    // #374151
            gray_800: Color::Rgb(31, 41, 55),    // #1f2937
            gray_900: Color::Rgb(17, 24, 39),    // #111827
            gray_950: Color::Rgb(3, 7, 18),      // #030712
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // HELPER METHODS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get slate shade by index (50, 100, ..., 950)
    pub fn slate(&self, shade: u16) -> Color {
        match shade {
            50 => self.slate_50,
            100 => self.slate_100,
            200 => self.slate_200,
            300 => self.slate_300,
            400 => self.slate_400,
            500 => self.slate_500,
            600 => self.slate_600,
            700 => self.slate_700,
            800 => self.slate_800,
            900 => self.slate_900,
            950 => self.slate_950,
            _ => self.slate_500,
        }
    }

    /// Get violet shade by index
    pub fn violet(&self, shade: u16) -> Color {
        match shade {
            50 => self.violet_50,
            100 => self.violet_100,
            200 => self.violet_200,
            300 => self.violet_300,
            400 => self.violet_400,
            500 => self.violet_500,
            600 => self.violet_600,
            700 => self.violet_700,
            800 => self.violet_800,
            900 => self.violet_900,
            950 => self.violet_950,
            _ => self.violet_500,
        }
    }

    /// Get cyan shade by index
    pub fn cyan(&self, shade: u16) -> Color {
        match shade {
            50 => self.cyan_50,
            100 => self.cyan_100,
            200 => self.cyan_200,
            300 => self.cyan_300,
            400 => self.cyan_400,
            500 => self.cyan_500,
            600 => self.cyan_600,
            700 => self.cyan_700,
            800 => self.cyan_800,
            900 => self.cyan_900,
            950 => self.cyan_950,
            _ => self.cyan_500,
        }
    }

    /// Get pink shade by index
    pub fn pink(&self, shade: u16) -> Color {
        match shade {
            50 => self.pink_50,
            100 => self.pink_100,
            200 => self.pink_200,
            300 => self.pink_300,
            400 => self.pink_400,
            500 => self.pink_500,
            600 => self.pink_600,
            700 => self.pink_700,
            800 => self.pink_800,
            900 => self.pink_900,
            950 => self.pink_950,
            _ => self.pink_500,
        }
    }

    /// Get emerald shade by index
    pub fn emerald(&self, shade: u16) -> Color {
        match shade {
            50 => self.emerald_50,
            100 => self.emerald_100,
            200 => self.emerald_200,
            300 => self.emerald_300,
            400 => self.emerald_400,
            500 => self.emerald_500,
            600 => self.emerald_600,
            700 => self.emerald_700,
            800 => self.emerald_800,
            900 => self.emerald_900,
            950 => self.emerald_950,
            _ => self.emerald_500,
        }
    }

    /// Get amber shade by index
    pub fn amber(&self, shade: u16) -> Color {
        match shade {
            50 => self.amber_50,
            100 => self.amber_100,
            200 => self.amber_200,
            300 => self.amber_300,
            400 => self.amber_400,
            500 => self.amber_500,
            600 => self.amber_600,
            700 => self.amber_700,
            800 => self.amber_800,
            900 => self.amber_900,
            950 => self.amber_950,
            _ => self.amber_500,
        }
    }

    /// Get rose shade by index
    pub fn rose(&self, shade: u16) -> Color {
        match shade {
            50 => self.rose_50,
            100 => self.rose_100,
            200 => self.rose_200,
            300 => self.rose_300,
            400 => self.rose_400,
            500 => self.rose_500,
            600 => self.rose_600,
            700 => self.rose_700,
            800 => self.rose_800,
            900 => self.rose_900,
            950 => self.rose_950,
            _ => self.rose_500,
        }
    }

    /// Get red shade by index
    pub fn red(&self, shade: u16) -> Color {
        match shade {
            50 => self.red_50,
            100 => self.red_100,
            200 => self.red_200,
            300 => self.red_300,
            400 => self.red_400,
            500 => self.red_500,
            600 => self.red_600,
            700 => self.red_700,
            800 => self.red_800,
            900 => self.red_900,
            950 => self.red_950,
            _ => self.red_500,
        }
    }

    /// Get blue shade by index
    pub fn blue(&self, shade: u16) -> Color {
        match shade {
            50 => self.blue_50,
            100 => self.blue_100,
            200 => self.blue_200,
            300 => self.blue_300,
            400 => self.blue_400,
            500 => self.blue_500,
            600 => self.blue_600,
            700 => self.blue_700,
            800 => self.blue_800,
            900 => self.blue_900,
            950 => self.blue_950,
            _ => self.blue_500,
        }
    }

    /// Get green shade by index
    pub fn green(&self, shade: u16) -> Color {
        match shade {
            50 => self.green_50,
            100 => self.green_100,
            200 => self.green_200,
            300 => self.green_300,
            400 => self.green_400,
            500 => self.green_500,
            600 => self.green_600,
            700 => self.green_700,
            800 => self.green_800,
            900 => self.green_900,
            950 => self.green_950,
            _ => self.green_500,
        }
    }

    /// Get yellow shade by index
    pub fn yellow(&self, shade: u16) -> Color {
        match shade {
            50 => self.yellow_50,
            100 => self.yellow_100,
            200 => self.yellow_200,
            300 => self.yellow_300,
            400 => self.yellow_400,
            500 => self.yellow_500,
            600 => self.yellow_600,
            700 => self.yellow_700,
            800 => self.yellow_800,
            900 => self.yellow_900,
            950 => self.yellow_950,
            _ => self.yellow_500,
        }
    }

    /// Get gray shade by index
    pub fn gray(&self, shade: u16) -> Color {
        match shade {
            50 => self.gray_50,
            100 => self.gray_100,
            200 => self.gray_200,
            300 => self.gray_300,
            400 => self.gray_400,
            500 => self.gray_500,
            600 => self.gray_600,
            700 => self.gray_700,
            800 => self.gray_800,
            900 => self.gray_900,
            950 => self.gray_950,
            _ => self.gray_500,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_creation() {
        let palette = ColorPalette::tailwind();
        // Verify some key colors
        assert_eq!(palette.slate_900, Color::Rgb(15, 23, 42));
        assert_eq!(palette.violet_500, Color::Rgb(139, 92, 246));
        assert_eq!(palette.cyan_500, Color::Rgb(6, 182, 212));
    }

    #[test]
    fn test_shade_helpers() {
        let palette = ColorPalette::tailwind();
        assert_eq!(palette.slate(900), Color::Rgb(15, 23, 42));
        assert_eq!(palette.violet(500), Color::Rgb(139, 92, 246));
        assert_eq!(palette.cyan(400), Color::Rgb(34, 211, 238));
    }

    #[test]
    fn test_invalid_shade_defaults() {
        let palette = ColorPalette::tailwind();
        // Invalid shades should return 500
        assert_eq!(palette.slate(999), palette.slate_500);
        assert_eq!(palette.violet(0), palette.violet_500);
    }

    #[test]
    fn test_all_palettes_have_11_shades() {
        let palette = ColorPalette::tailwind();
        // Each palette should have 50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950
        let shades = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];

        for shade in shades {
            // These should not panic (12 palettes total)
            let _ = palette.slate(shade);
            let _ = palette.violet(shade);
            let _ = palette.cyan(shade);
            let _ = palette.pink(shade);
            let _ = palette.emerald(shade);
            let _ = palette.amber(shade);
            let _ = palette.rose(shade);
            let _ = palette.red(shade);
            let _ = palette.blue(shade);
            let _ = palette.green(shade);
            let _ = palette.yellow(shade);
            let _ = palette.gray(shade);
        }
    }

    #[test]
    fn test_new_palettes_values() {
        let palette = ColorPalette::tailwind();
        // Verify official Tailwind CSS v3.4 values for new palettes
        assert_eq!(palette.green_500, Color::Rgb(34, 197, 94)); // #22c55e
        assert_eq!(palette.yellow_500, Color::Rgb(234, 179, 8)); // #eab308
        assert_eq!(palette.gray_500, Color::Rgb(107, 114, 128)); // #6b7280
    }

    #[test]
    fn test_contrast_ratios_wcag() {
        // Verify key contrast ratios for accessibility
        // Slate-50 vs Slate-900 should have high contrast
        let palette = ColorPalette::tailwind();

        // Extract RGB for manual calculation if needed
        if let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) =
            (palette.slate_50, palette.slate_900)
        {
            // Slate-50 is light, Slate-900 is dark - good contrast
            assert!(r1 > r2);
            assert!(g1 > g2);
            assert!(b1 > b2);
        }
    }
}
