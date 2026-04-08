// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # Cosmic Tailwind Design System 🦋🌌
//!
//! Modular token-based design system for Nika TUI.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │  Level 1: ColorPalette      Raw Tailwind colors (Slate, Violet, etc.)  │
//! │  Level 2: SemanticColors    Purpose-driven colors (bg, text, verbs)    │
//! │  Level 3: ComponentTokens   UI element colors (button, panel, etc.)    │
//! │  Level 4: ThemeVariants     Dark/Light/Cosmic configurations           │
//! │  Level 5: TokenResolver     Runtime resolution with mode detection     │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Cosmic Tailwind Philosophy
//!
//! - **Slate palette**: Blue-tinted neutrals (not pure gray) for depth
//! - **Violet primary**: 🦋 Nika brand accent
//! - **Cyan secondary**: 🌌 Cosmic glow effects
//! - **Pink accent**: Nebula highlights
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika_tui::tokens::{CosmicTheme, TokenResolver};
//!
//! let resolver = TokenResolver::cosmic_dark();
//! let bg = resolver.bg_primary();
//! let text = resolver.text_primary();
//! ```

pub mod colors;
pub mod compat;
pub mod semantic;

pub use colors::*;
pub use compat::*;
pub use semantic::*;

use ratatui::style::Color;

// ═══════════════════════════════════════════════════════════════════════════════
// COSMIC THEME PRESETS
// ═══════════════════════════════════════════════════════════════════════════════

/// Cosmic theme variant selector
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CosmicVariant {
    /// 🌌 Cosmic Dark - Deep space with nebula accents
    #[default]
    CosmicDark,
    /// 🌅 Cosmic Light - Aurora borealis on light background
    CosmicLight,
    /// 🦋 Cosmic Violet - Full violet immersion
    CosmicViolet,
}

impl CosmicVariant {
    /// Create theme resolver for this variant
    pub fn resolver(&self) -> TokenResolver {
        match self {
            Self::CosmicDark => TokenResolver::cosmic_dark(),
            Self::CosmicLight => TokenResolver::cosmic_light(),
            Self::CosmicViolet => TokenResolver::cosmic_violet(),
        }
    }

    /// Cycle to next variant
    pub fn cycle(&self) -> Self {
        match self {
            Self::CosmicDark => Self::CosmicLight,
            Self::CosmicLight => Self::CosmicViolet,
            Self::CosmicViolet => Self::CosmicDark,
        }
    }

    /// Display label with emoji
    pub fn label(&self) -> &'static str {
        match self {
            Self::CosmicDark => "🌌 Cosmic Dark",
            Self::CosmicLight => "🌅 Cosmic Light",
            Self::CosmicViolet => "🦋 Cosmic Violet",
        }
    }

    /// Create variant from index (0=Dark, 1=Light, 2=Violet)
    ///
    /// Used by Settings view for direct theme selection via `[1][2][3]` keys.
    pub fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::CosmicDark),
            1 => Some(Self::CosmicLight),
            2 => Some(Self::CosmicViolet),
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TOKEN RESOLVER (Level 5)
// ═══════════════════════════════════════════════════════════════════════════════

/// Runtime token resolver with cached semantic colors
///
/// Resolves design tokens to actual colors based on theme variant.
/// Caches computed values for performance.
#[derive(Debug, Clone)]
pub struct TokenResolver {
    palette: ColorPalette,
    semantic: SemanticColors,
    variant: CosmicVariant,
}

impl Default for TokenResolver {
    fn default() -> Self {
        Self::cosmic_dark()
    }
}

impl TokenResolver {
    /// Create Cosmic Dark resolver 🌌
    pub fn cosmic_dark() -> Self {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_dark(&palette);
        Self {
            palette,
            semantic,
            variant: CosmicVariant::CosmicDark,
        }
    }

    /// Create Cosmic Light resolver 🌅
    pub fn cosmic_light() -> Self {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_light(&palette);
        Self {
            palette,
            semantic,
            variant: CosmicVariant::CosmicLight,
        }
    }

    /// Create Cosmic Violet resolver 🦋
    pub fn cosmic_violet() -> Self {
        let palette = ColorPalette::tailwind();
        let semantic = SemanticColors::cosmic_violet(&palette);
        Self {
            palette,
            semantic,
            variant: CosmicVariant::CosmicViolet,
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // BACKGROUND TOKENS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Primary background (main canvas)
    pub fn bg_primary(&self) -> Color {
        self.semantic.bg_primary
    }

    /// Secondary background (elevated surfaces)
    pub fn bg_secondary(&self) -> Color {
        self.semantic.bg_secondary
    }

    /// Tertiary background (cards, panels)
    pub fn bg_tertiary(&self) -> Color {
        self.semantic.bg_tertiary
    }

    /// Hover state background
    pub fn bg_hover(&self) -> Color {
        self.semantic.bg_hover
    }

    /// Active/selected state background
    pub fn bg_active(&self) -> Color {
        self.semantic.bg_active
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // TEXT TOKENS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Primary text (main content)
    pub fn text_primary(&self) -> Color {
        self.semantic.text_primary
    }

    /// Secondary text (descriptions, labels)
    pub fn text_secondary(&self) -> Color {
        self.semantic.text_secondary
    }

    /// Muted text (hints, placeholders)
    pub fn text_muted(&self) -> Color {
        self.semantic.text_muted
    }

    /// Disabled text
    pub fn text_disabled(&self) -> Color {
        self.semantic.text_disabled
    }

    /// Inverse text (on accent backgrounds)
    pub fn text_inverse(&self) -> Color {
        self.semantic.text_inverse
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // BORDER TOKENS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Default border
    pub fn border_default(&self) -> Color {
        self.semantic.border_default
    }

    /// Focused border (with accent)
    pub fn border_focused(&self) -> Color {
        self.semantic.border_focused
    }

    /// Subtle border (dividers)
    pub fn border_subtle(&self) -> Color {
        self.semantic.border_subtle
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // ACCENT TOKENS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Primary accent (🦋 Violet)
    pub fn accent_primary(&self) -> Color {
        self.semantic.accent_primary
    }

    /// Secondary accent (🌌 Cyan)
    pub fn accent_secondary(&self) -> Color {
        self.semantic.accent_secondary
    }

    /// Tertiary accent (Pink nebula)
    pub fn accent_tertiary(&self) -> Color {
        self.semantic.accent_tertiary
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // STATUS TOKENS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Success color (green)
    pub fn status_success(&self) -> Color {
        self.semantic.status_success
    }

    /// Warning color (amber)
    pub fn status_warning(&self) -> Color {
        self.semantic.status_warning
    }

    /// Error color (red)
    pub fn status_error(&self) -> Color {
        self.semantic.status_error
    }

    /// Info color (blue)
    pub fn status_info(&self) -> Color {
        self.semantic.status_info
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // VERB TOKENS (5 semantic verbs)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Infer verb color (Violet)
    pub fn verb_infer(&self) -> Color {
        self.semantic.verb_infer
    }

    /// Exec verb color (Amber)
    pub fn verb_exec(&self) -> Color {
        self.semantic.verb_exec
    }

    /// Fetch verb color (Cyan)
    pub fn verb_fetch(&self) -> Color {
        self.semantic.verb_fetch
    }

    /// Invoke verb color (Emerald)
    pub fn verb_invoke(&self) -> Color {
        self.semantic.verb_invoke
    }

    /// Agent verb color (Rose)
    pub fn verb_agent(&self) -> Color {
        self.semantic.verb_agent
    }

    /// Get verb color by name
    pub fn verb_color(&self, verb: &str) -> Color {
        match verb.to_lowercase().as_str() {
            "infer" => self.verb_infer(),
            "exec" => self.verb_exec(),
            "fetch" => self.verb_fetch(),
            "invoke" => self.verb_invoke(),
            "agent" => self.verb_agent(),
            _ => self.text_muted(),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // RAW PALETTE ACCESS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Access raw color palette for custom combinations
    pub fn palette(&self) -> &ColorPalette {
        &self.palette
    }

    /// Access semantic colors
    pub fn semantic(&self) -> &SemanticColors {
        &self.semantic
    }

    /// Get current variant
    pub fn variant(&self) -> CosmicVariant {
        self.variant
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosmic_dark_resolver() {
        let resolver = TokenResolver::cosmic_dark();
        assert_eq!(resolver.variant(), CosmicVariant::CosmicDark);
        // Abyss Dark background
        assert_eq!(resolver.bg_primary(), Color::Rgb(8, 8, 14));
    }

    #[test]
    fn test_cosmic_light_resolver() {
        let resolver = TokenResolver::cosmic_light();
        assert_eq!(resolver.variant(), CosmicVariant::CosmicLight);
        // Slate-50 background
        assert_eq!(resolver.bg_primary(), Color::Rgb(248, 250, 252));
    }

    #[test]
    fn test_cosmic_violet_resolver() {
        let resolver = TokenResolver::cosmic_violet();
        assert_eq!(resolver.variant(), CosmicVariant::CosmicViolet);
    }

    #[test]
    fn test_verb_colors() {
        let resolver = TokenResolver::cosmic_dark();
        // Neon purple for infer
        assert_eq!(resolver.verb_infer(), Color::Rgb(180, 120, 255));
        // Neon peach for exec
        assert_eq!(resolver.verb_exec(), Color::Rgb(255, 160, 80));
    }

    #[test]
    fn test_variant_cycle() {
        let v = CosmicVariant::CosmicDark;
        assert_eq!(v.cycle(), CosmicVariant::CosmicLight);
        assert_eq!(v.cycle().cycle(), CosmicVariant::CosmicViolet);
        assert_eq!(v.cycle().cycle().cycle(), CosmicVariant::CosmicDark);
    }

    #[test]
    fn test_variant_labels() {
        assert_eq!(CosmicVariant::CosmicDark.label(), "🌌 Cosmic Dark");
        assert_eq!(CosmicVariant::CosmicLight.label(), "🌅 Cosmic Light");
        assert_eq!(CosmicVariant::CosmicViolet.label(), "🦋 Cosmic Violet");
    }

    #[test]
    fn test_variant_from_index_valid() {
        assert_eq!(
            CosmicVariant::from_index(0),
            Some(CosmicVariant::CosmicDark)
        );
        assert_eq!(
            CosmicVariant::from_index(1),
            Some(CosmicVariant::CosmicLight)
        );
        assert_eq!(
            CosmicVariant::from_index(2),
            Some(CosmicVariant::CosmicViolet)
        );
    }

    #[test]
    fn test_variant_from_index_invalid() {
        assert_eq!(CosmicVariant::from_index(3), None);
        assert_eq!(CosmicVariant::from_index(255), None);
    }
}
