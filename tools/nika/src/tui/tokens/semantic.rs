//! # Semantic Colors (Level 2)
//!
//! Purpose-driven color assignments derived from the raw palette.
//!
//! ## Philosophy
//!
//! Semantic colors abstract raw palette values into meaningful categories:
//! - **Background**: Canvas, surfaces, states
//! - **Text**: Primary, secondary, muted, disabled
//! - **Border**: Default, focused, subtle
//! - **Accent**: Primary (violet), secondary (cyan), tertiary (pink)
//! - **Status**: Success, warning, error, info
//! - **Verb**: 5 semantic verbs with distinct colors

use super::ColorPalette;
use ratatui::style::Color;

// ═══════════════════════════════════════════════════════════════════════════════
// SEMANTIC COLORS (Level 2)
// ═══════════════════════════════════════════════════════════════════════════════

/// Semantic color assignments for a theme variant
///
/// Maps abstract concepts (bg, text, accent) to concrete colors.
#[derive(Debug, Clone)]
pub struct SemanticColors {
    // ═══════════════════════════════════════════════════════════════════════════
    // BACKGROUNDS
    // ═══════════════════════════════════════════════════════════════════════════
    /// Primary background (main canvas)
    pub bg_primary: Color,
    /// Secondary background (elevated surfaces, sidebars)
    pub bg_secondary: Color,
    /// Tertiary background (cards, panels, modals)
    pub bg_tertiary: Color,
    /// Hover state background
    pub bg_hover: Color,
    /// Active/selected state background
    pub bg_active: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // TEXT
    // ═══════════════════════════════════════════════════════════════════════════
    /// Primary text (main content, headings)
    pub text_primary: Color,
    /// Secondary text (descriptions, labels)
    pub text_secondary: Color,
    /// Muted text (hints, placeholders)
    pub text_muted: Color,
    /// Disabled text
    pub text_disabled: Color,
    /// Inverse text (on accent backgrounds)
    pub text_inverse: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // BORDERS
    // ═══════════════════════════════════════════════════════════════════════════
    /// Default border (panels, inputs)
    pub border_default: Color,
    /// Focused border (active elements)
    pub border_focused: Color,
    /// Subtle border (dividers, separators)
    pub border_subtle: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // ACCENTS (Cosmic Theme)
    // ═══════════════════════════════════════════════════════════════════════════
    /// Primary accent (🦋 Violet - Nika brand)
    pub accent_primary: Color,
    /// Secondary accent (🌌 Cyan - cosmic glow)
    pub accent_secondary: Color,
    /// Tertiary accent (Pink - nebula highlights)
    pub accent_tertiary: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // STATUS
    // ═══════════════════════════════════════════════════════════════════════════
    /// Success color
    pub status_success: Color,
    /// Warning color
    pub status_warning: Color,
    /// Error color
    pub status_error: Color,
    /// Info color
    pub status_info: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // VERB COLORS (5 semantic verbs)
    // ═══════════════════════════════════════════════════════════════════════════
    /// Infer verb (Violet) - LLM generation
    pub verb_infer: Color,
    /// Exec verb (Amber) - Shell execution
    pub verb_exec: Color,
    /// Fetch verb (Cyan) - HTTP requests
    pub verb_fetch: Color,
    /// Invoke verb (Emerald) - MCP tool calls
    pub verb_invoke: Color,
    /// Agent verb (Rose) - Multi-turn agentic loop
    pub verb_agent: Color,

    // ═══════════════════════════════════════════════════════════════════════════
    // SCROLLBAR
    // ═══════════════════════════════════════════════════════════════════════════
    /// Scrollbar thumb (main indicator)
    pub scrollbar_thumb: Color,
    /// Scrollbar track (background)
    pub scrollbar_track: Color,
    /// Scrollbar arrows
    pub scrollbar_arrows: Color,
}

impl SemanticColors {
    /// Create Cosmic Dark semantic colors 🌌
    ///
    /// Deep space aesthetic with violet/cyan accents
    pub fn cosmic_dark(palette: &ColorPalette) -> Self {
        Self {
            // Backgrounds - Slate for depth
            bg_primary: palette.slate_900,   // #0f172a - Deep space
            bg_secondary: palette.slate_800, // #1e293b - Elevated
            bg_tertiary: palette.slate_700,  // #334155 - Cards
            bg_hover: palette.slate_700,     // #334155 - Hover
            bg_active: palette.violet_900,   // #4c1d95 - Active (violet tint)

            // Text - High contrast for readability
            text_primary: palette.slate_50,   // #f8fafc - Bright white
            text_secondary: palette.slate_300, // #cbd5e1 - Muted
            text_muted: palette.slate_500,    // #64748b - Hints
            text_disabled: palette.slate_600, // #475569 - Disabled
            text_inverse: palette.slate_900,  // #0f172a - On light

            // Borders - Subtle definition
            border_default: palette.slate_700, // #334155
            border_focused: palette.violet_500, // #8b5cf6 - Violet focus
            border_subtle: palette.slate_800,  // #1e293b

            // Accents - Cosmic palette
            accent_primary: palette.violet_500,   // #8b5cf6 - 🦋 Brand
            accent_secondary: palette.cyan_400,   // #22d3ee - 🌌 Glow
            accent_tertiary: palette.pink_500,    // #ec4899 - Nebula

            // Status - Clear semantics
            status_success: palette.emerald_500, // #10b981
            status_warning: palette.amber_500,   // #f59e0b
            status_error: palette.red_500,       // #ef4444
            status_info: palette.blue_500,       // #3b82f6

            // Verbs - Distinct colors per action
            verb_infer: palette.violet_500,  // #8b5cf6 - LLM
            verb_exec: palette.amber_500,    // #f59e0b - Shell
            verb_fetch: palette.cyan_500,    // #06b6d4 - HTTP
            verb_invoke: palette.emerald_500, // #10b981 - MCP
            verb_agent: palette.rose_500,    // #f43f5e - Agent

            // Scrollbar - Accent-colored
            scrollbar_thumb: palette.violet_600,  // #7c3aed
            scrollbar_track: palette.slate_800,   // #1e293b
            scrollbar_arrows: palette.slate_400,  // #94a3b8
        }
    }

    /// Create Cosmic Light semantic colors 🌅
    ///
    /// Aurora borealis on light background
    pub fn cosmic_light(palette: &ColorPalette) -> Self {
        Self {
            // Backgrounds - Light slate
            bg_primary: palette.slate_50,    // #f8fafc - Clean white
            bg_secondary: palette.slate_100, // #f1f5f9 - Elevated
            bg_tertiary: palette.slate_200,  // #e2e8f0 - Cards
            bg_hover: palette.slate_100,     // #f1f5f9 - Hover
            bg_active: palette.violet_100,   // #ede9fe - Active (violet tint)

            // Text - Dark for contrast
            text_primary: palette.slate_900,  // #0f172a - Dark
            text_secondary: palette.slate_600, // #475569 - Muted
            text_muted: palette.slate_400,    // #94a3b8 - Hints
            text_disabled: palette.slate_300, // #cbd5e1 - Disabled
            text_inverse: palette.slate_50,   // #f8fafc - On dark

            // Borders - Visible definition
            border_default: palette.slate_300, // #cbd5e1
            border_focused: palette.violet_600, // #7c3aed - Violet focus
            border_subtle: palette.slate_200,  // #e2e8f0

            // Accents - Deeper for contrast
            accent_primary: palette.violet_600,   // #7c3aed - 🦋 Brand
            accent_secondary: palette.cyan_600,   // #0891b2 - 🌌 Glow
            accent_tertiary: palette.pink_600,    // #db2777 - Nebula

            // Status - Darker for light mode
            status_success: palette.emerald_600, // #059669
            status_warning: palette.amber_600,   // #d97706
            status_error: palette.red_600,       // #dc2626
            status_info: palette.blue_600,       // #2563eb

            // Verbs - Slightly darker for readability
            verb_infer: palette.violet_600,  // #7c3aed
            verb_exec: palette.amber_600,    // #d97706
            verb_fetch: palette.cyan_600,    // #0891b2
            verb_invoke: palette.emerald_600, // #059669
            verb_agent: palette.rose_600,    // #e11d48

            // Scrollbar - Subtle
            scrollbar_thumb: palette.slate_400,  // #94a3b8
            scrollbar_track: palette.slate_200,  // #e2e8f0
            scrollbar_arrows: palette.slate_500, // #64748b
        }
    }

    /// Create Cosmic Violet semantic colors 🦋
    ///
    /// Full violet immersion for Nika brand
    pub fn cosmic_violet(palette: &ColorPalette) -> Self {
        Self {
            // Backgrounds - Violet-tinted
            bg_primary: palette.violet_950,  // #2e1065 - Deep violet
            bg_secondary: palette.violet_900, // #4c1d95 - Elevated
            bg_tertiary: palette.violet_800, // #5b21b6 - Cards
            bg_hover: palette.violet_800,    // #5b21b6 - Hover
            bg_active: palette.cyan_900,     // #164e63 - Active (cyan pop)

            // Text - Light for contrast
            text_primary: palette.violet_50,   // #f5f3ff - Bright
            text_secondary: palette.violet_200, // #ddd6fe - Muted
            text_muted: palette.violet_400,    // #a78bfa - Hints
            text_disabled: palette.violet_600, // #7c3aed - Disabled
            text_inverse: palette.violet_950,  // #2e1065 - On light

            // Borders - Violet spectrum
            border_default: palette.violet_700, // #6d28d9
            border_focused: palette.cyan_400,   // #22d3ee - Cyan focus (contrast)
            border_subtle: palette.violet_800,  // #5b21b6

            // Accents - Complementary
            accent_primary: palette.cyan_400,     // #22d3ee - Inverse accent
            accent_secondary: palette.pink_400,   // #f472b6 - Warm
            accent_tertiary: palette.violet_300,  // #c4b5fd - Soft

            // Status - Adjusted for violet bg
            status_success: palette.emerald_400, // #34d399 - Brighter
            status_warning: palette.amber_400,   // #fbbf24 - Brighter
            status_error: palette.rose_400,      // #fb7185 - Brighter
            status_info: palette.cyan_400,       // #22d3ee - Brighter

            // Verbs - High contrast on violet
            verb_infer: palette.violet_300,  // #c4b5fd - Lighter violet
            verb_exec: palette.amber_400,    // #fbbf24
            verb_fetch: palette.cyan_400,    // #22d3ee
            verb_invoke: palette.emerald_400, // #34d399
            verb_agent: palette.rose_400,    // #fb7185

            // Scrollbar - Cyan accent
            scrollbar_thumb: palette.cyan_500,   // #06b6d4
            scrollbar_track: palette.violet_800, // #5b21b6
            scrollbar_arrows: palette.violet_200, // #ddd6fe
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // HELPER METHODS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get verb color by name
    pub fn verb(&self, name: &str) -> Color {
        match name.to_lowercase().as_str() {
            "infer" => self.verb_infer,
            "exec" => self.verb_exec,
            "fetch" => self.verb_fetch,
            "invoke" => self.verb_invoke,
            "agent" => self.verb_agent,
            _ => self.text_muted,
        }
    }

    /// Get status color by state
    pub fn status(&self, state: &str) -> Color {
        match state.to_lowercase().as_str() {
            "success" | "completed" | "done" => self.status_success,
            "warning" | "pending" | "running" => self.status_warning,
            "error" | "failed" => self.status_error,
            "info" => self.status_info,
            _ => self.text_muted,
        }
    }

    /// Check if this is a dark theme (for contrast calculations)
    pub fn is_dark(&self) -> bool {
        // Extract RGB from bg_primary
        if let Color::Rgb(r, g, b) = self.bg_primary {
            // Calculate relative luminance
            let luminance = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
            luminance < 128
        } else {
            true // Default to dark
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
    fn test_cosmic_dark() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_dark(&palette);

        assert_eq!(semantic.bg_primary, palette.slate_900);
        assert_eq!(semantic.accent_primary, palette.violet_500);
        assert!(semantic.is_dark());
    }

    #[test]
    fn test_cosmic_light() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_light(&palette);

        assert_eq!(semantic.bg_primary, palette.slate_50);
        assert_eq!(semantic.accent_primary, palette.violet_600);
        assert!(!semantic.is_dark());
    }

    #[test]
    fn test_cosmic_violet() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_violet(&palette);

        assert_eq!(semantic.bg_primary, palette.violet_950);
        assert!(semantic.is_dark());
    }

    #[test]
    fn test_verb_helper() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_dark(&palette);

        assert_eq!(semantic.verb("infer"), palette.violet_500);
        assert_eq!(semantic.verb("exec"), palette.amber_500);
        assert_eq!(semantic.verb("fetch"), palette.cyan_500);
        assert_eq!(semantic.verb("invoke"), palette.emerald_500);
        assert_eq!(semantic.verb("agent"), palette.rose_500);
        assert_eq!(semantic.verb("unknown"), semantic.text_muted);
    }

    #[test]
    fn test_status_helper() {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_dark(&palette);

        assert_eq!(semantic.status("success"), palette.emerald_500);
        assert_eq!(semantic.status("completed"), palette.emerald_500);
        assert_eq!(semantic.status("warning"), palette.amber_500);
        assert_eq!(semantic.status("error"), palette.red_500);
        assert_eq!(semantic.status("info"), palette.blue_500);
    }

    #[test]
    fn test_all_themes_have_complete_verbs() {
        let palette = ColorPalette::tailwind();

        for semantic in [
            SemanticColors::cosmic_dark(&palette),
            SemanticColors::cosmic_light(&palette),
            SemanticColors::cosmic_violet(&palette),
        ] {
            // All verb colors should be different
            let verbs = [
                semantic.verb_infer,
                semantic.verb_exec,
                semantic.verb_fetch,
                semantic.verb_invoke,
                semantic.verb_agent,
            ];

            for (i, v1) in verbs.iter().enumerate() {
                for (j, v2) in verbs.iter().enumerate() {
                    if i != j {
                        assert_ne!(v1, v2, "Verb colors must be distinct");
                    }
                }
            }
        }
    }
}
