// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # Cosmic Theme Bridge Layer
//!
//! Bridges the new `TokenResolver` design system with the `Theme` API.
//!
//! This module provides `CosmicTheme`, an adapter that wraps `TokenResolver`
//! while exposing a `Theme`-compatible API.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
//! │   CosmicTheme    │ ──► │   TokenResolver  │ ──► │  SemanticColors  │
//! │   (Adapter)      │     │      │     │  ColorPalette    │
//! └──────────────────┘     └──────────────────┘     └──────────────────┘
//!          │
//!          ▼
//! ┌──────────────────┐
//! │      Theme       │ (Convenience API)
//! │                  │
//! └──────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika_tui::cosmic_theme::CosmicTheme;
//! use nika_tui::tokens::CosmicVariant;
//!
//! // Create with default variant
//! let mut cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);
//!
//! // Get Theme
//! let theme = cosmic.as_theme();
//!
//! // Get resolver for new code
//! let resolver = cosmic.resolver();
//!
//! // Cycle through variants
//! cosmic.cycle(); // Dark → Light → Violet → Dark
//! ```

use std::cell::OnceCell;

use ratatui::style::Color;

use super::theme::{Theme, ThemeMode};
use super::tokens::{CosmicVariant, TokenResolver};

// ═══════════════════════════════════════════════════════════════════════════════
// COLOR HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

/// Lighten an RGB color by a multiplicative factor (>1.0 = brighter).
///
/// Non-RGB colors pass through unchanged.
fn lighten_color(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor).min(255.0) as u8,
            (g as f32 * factor).min(255.0) as u8,
            (b as f32 * factor).min(255.0) as u8,
        ),
        other => other,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// COSMIC THEME ADAPTER
// ═══════════════════════════════════════════════════════════════════════════════

/// Bridge between TokenResolver and Theme API.
///
/// `CosmicTheme` wraps a `TokenResolver` and provides both:
/// - `Theme` access via `as_theme()` for existing widgets
/// - Direct `TokenResolver` access via `resolver()` for new code
///
/// This enables incremental adoption without breaking existing usages.
///
/// # Performance
///
/// The `as_theme()` method caches its result using `OnceCell`. The cache is
/// automatically invalidated when `cycle()` or `set_variant()` is called.
#[derive(Debug)]
pub struct CosmicTheme {
    resolver: TokenResolver,
    variant: CosmicVariant,
    /// Cached Theme for hot-path access (invalidated on variant change)
    cached_theme: OnceCell<Theme>,
}

impl Clone for CosmicTheme {
    fn clone(&self) -> Self {
        Self {
            resolver: self.resolver.clone(),
            variant: self.variant,
            cached_theme: OnceCell::new(), // Fresh cache for clone
        }
    }
}

impl Default for CosmicTheme {
    fn default() -> Self {
        Self::new(CosmicVariant::default())
    }
}

impl CosmicTheme {
    /// Create a new CosmicTheme with the given variant.
    ///
    /// # Arguments
    ///
    /// * `variant` - The cosmic variant (Dark, Light, or Violet)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);
    /// ```
    pub fn new(variant: CosmicVariant) -> Self {
        Self {
            resolver: variant.resolver(),
            variant,
            cached_theme: OnceCell::new(),
        }
    }

    /// Get the current variant.
    pub fn variant(&self) -> CosmicVariant {
        self.variant
    }

    /// Get Theme for existing widgets.
    ///
    /// This creates a `Theme` struct populated from the TokenResolver's
    /// semantic colors. Use this for existing widgets that expect `&Theme`.
    ///
    /// # Performance
    ///
    /// This method caches the Theme using `OnceCell`. Subsequent calls
    /// return a clone of the cached value. The cache is invalidated
    /// when `cycle()` or `set_variant()` is called.
    pub fn as_theme(&self) -> Theme {
        self.cached_theme
            .get_or_init(|| Theme::from_resolver(&self.resolver))
            .clone()
    }

    /// Get resolver for new code.
    ///
    /// New widgets should use the resolver directly for:
    /// - Semantic color access (bg_primary, text_primary, etc.)
    /// - Verb colors
    /// - Status colors
    pub fn resolver(&self) -> &TokenResolver {
        &self.resolver
    }

    /// Cycle to the next theme variant.
    ///
    /// Order: CosmicDark → CosmicLight → CosmicViolet → CosmicDark
    ///
    /// Invalidates the cached Theme.
    pub fn cycle(&mut self) {
        self.variant = self.variant.cycle();
        self.resolver = self.variant.resolver();
        self.cached_theme.take(); // Invalidate cache
    }

    /// Set a specific variant.
    ///
    /// Invalidates the cached Theme.
    pub fn set_variant(&mut self, variant: CosmicVariant) {
        self.variant = variant;
        self.resolver = variant.resolver();
        self.cached_theme.take(); // Invalidate cache
    }

    /// Get display label for current variant.
    pub fn label(&self) -> &'static str {
        self.variant.label()
    }

    /// Check if current variant is dark.
    pub fn is_dark(&self) -> bool {
        self.resolver.semantic().is_dark()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// THEME MODE CONVERSION
// ═══════════════════════════════════════════════════════════════════════════════

impl From<ThemeMode> for CosmicVariant {
    /// Convert ThemeMode to CosmicVariant.
    ///
    /// Mapping:
    /// - Dark → CosmicDark
    /// - Light → CosmicLight
    /// - Solarized → CosmicDark (closest match)
    fn from(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => CosmicVariant::CosmicDark,
            ThemeMode::Light => CosmicVariant::CosmicLight,
            ThemeMode::Solarized => CosmicVariant::CosmicDark, // Closest match
        }
    }
}

impl From<CosmicVariant> for ThemeMode {
    /// Convert CosmicVariant to ThemeMode.
    ///
    /// Mapping:
    /// - CosmicDark → Dark
    /// - CosmicLight → Light
    /// - CosmicViolet → Dark (closest match)
    fn from(variant: CosmicVariant) -> Self {
        match variant {
            CosmicVariant::CosmicDark => ThemeMode::Dark,
            CosmicVariant::CosmicLight => ThemeMode::Light,
            CosmicVariant::CosmicViolet => ThemeMode::Dark, // Closest match
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// THEME FROM RESOLVER (Bridge Method)
// ═══════════════════════════════════════════════════════════════════════════════

impl Theme {
    /// Create Theme from TokenResolver (bridge method).
    ///
    /// Maps all 35+ Theme fields from the resolver's semantic colors.
    pub fn from_resolver(resolver: &TokenResolver) -> Self {
        let semantic = resolver.semantic();

        Self {
            // ═══ REALMS ═══
            // Using accent colors for realms (no direct mapping)
            realm_shared: semantic.accent_secondary, // Cyan for shared
            realm_org: semantic.status_success,      // Emerald for org

            // ═══ TRAITS (Data Origin) ═══
            trait_defined: semantic.text_muted, // Gray for defined
            trait_authored: semantic.accent_primary, // Violet for authored
            trait_imported: semantic.verb_exec, // Amber for imported
            trait_generated: semantic.status_success, // Emerald for generated
            trait_retrieved: semantic.accent_secondary, // Cyan for retrieved

            // ═══ TASK STATUS ═══
            status_pending: semantic.text_muted,
            status_running: semantic.status_warning,
            status_success: semantic.status_success,
            status_failed: semantic.status_error,
            status_paused: semantic.accent_secondary,

            // ═══ MISSION PHASES ═══
            phase_launch: semantic.accent_tertiary,    // Pink
            phase_orbital: semantic.status_info,       // Blue
            phase_rendezvous: semantic.accent_primary, // Violet

            // ═══ MCP TOOLS ═══
            mcp_describe: semantic.status_info,    // Blue
            mcp_context: semantic.accent_tertiary, // Pink
            mcp_search: semantic.verb_exec,        // Amber
            mcp_audit: semantic.accent_primary,    // Violet
            mcp_write: semantic.status_success,    // Emerald

            // ═══ UI ELEMENTS ═══
            border_normal: semantic.border_default,
            border_focused: semantic.border_focused,
            text: semantic.text_primary, // alias for selection
            text_primary: semantic.text_primary,
            text_secondary: semantic.text_secondary,
            text_muted: semantic.text_muted,
            background: semantic.bg_primary,
            highlight: semantic.accent_primary,
            selection: semantic.accent_secondary, // cyan for selection

            // ═══ GIT GUTTER ═══
            git_added: semantic.status_success, // Green for added lines
            git_modified: semantic.status_warning, // Yellow/Orange for modified
            git_deleted: semantic.status_error, // Red for deleted markers

            // ═══ SCROLLBAR ═══
            scrollbar_thumb: semantic.scrollbar_thumb,
            scrollbar_track: semantic.scrollbar_track,
            scrollbar_arrows: semantic.scrollbar_arrows,

            // ═══ DIAGNOSTICS ═══
            diagnostic_error: resolver.status_error(),
            diagnostic_warning: resolver.status_warning(),
            diagnostic_hint: resolver.status_info(),

            // ═══ POPUP / OVERLAY ═══
            popup_bg: resolver.bg_secondary(),
            popup_border: resolver.border_default(),
            popup_selected: resolver.bg_hover(),

            // ═══ VERB GLOW ═══
            verb_infer_glow: lighten_color(semantic.verb_infer, 1.3),
            verb_exec_glow: lighten_color(semantic.verb_exec, 1.3),
            verb_fetch_glow: lighten_color(semantic.verb_fetch, 1.3),
            verb_invoke_glow: lighten_color(semantic.verb_invoke, 1.3),
            verb_agent_glow: lighten_color(semantic.verb_agent, 1.3),

            // ═══ TREE WIDGET ═══
            tree_directory: Color::Rgb(80, 140, 255), // Electric blue
            tree_file: resolver.text_secondary(),
            tree_hidden: resolver.text_muted(),
            tree_ecosystem: Color::Rgb(255, 200, 0), // Amber
            tree_ecosystem_glow: Color::Rgb(255, 160, 80), // Neon peach
            tree_indent_guide: resolver.text_muted(),

            // ═══ SYNTAX HIGHLIGHTING ═══
            syntax_keyword: Color::Rgb(180, 120, 255), // Neon purple
            syntax_string: Color::Rgb(0, 230, 120),    // Neon green
            syntax_number: Color::Rgb(255, 160, 80),   // Neon peach
            syntax_comment: Color::Rgb(55, 60, 75),    // Dim overlay
            syntax_verb: Color::Rgb(180, 120, 255),    // Neon purple
            syntax_template: Color::Rgb(0, 220, 150),  // Neon teal
            syntax_mcp_server: Color::Rgb(0, 200, 220), // Neon cyan
            syntax_task_id: Color::Rgb(255, 200, 0),   // Amber

            // HEADER / STATUS BAR
            header_bg: if semantic.is_dark() {
                Color::Rgb(6, 6, 10) // Near-black (darker than canvas)
            } else {
                Color::Rgb(236, 239, 244) // Slate-150-ish (light theme)
            },
            status_parsing_a: semantic.accent_tertiary,
            status_parsing_b: semantic.accent_secondary,
            status_executing_a: semantic.status_warning,
            status_executing_b: semantic.verb_exec,
        }
    }

    /// Create Cosmic Dark theme (new default).
    pub fn cosmic_dark() -> Self {
        Self::from_resolver(&TokenResolver::cosmic_dark())
    }

    /// Create Cosmic Light theme.
    pub fn cosmic_light() -> Self {
        Self::from_resolver(&TokenResolver::cosmic_light())
    }

    /// Create Cosmic Violet theme.
    pub fn cosmic_violet() -> Self {
        Self::from_resolver(&TokenResolver::cosmic_violet())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_cosmic_theme_new_default() {
        let cosmic = CosmicTheme::default();
        assert_eq!(cosmic.variant(), CosmicVariant::CosmicDark);
    }

    #[test]
    fn test_cosmic_theme_new_with_variant() {
        let cosmic = CosmicTheme::new(CosmicVariant::CosmicLight);
        assert_eq!(cosmic.variant(), CosmicVariant::CosmicLight);
    }

    #[test]
    fn test_cosmic_theme_cycle() {
        let mut cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);

        cosmic.cycle();
        assert_eq!(cosmic.variant(), CosmicVariant::CosmicLight);

        cosmic.cycle();
        assert_eq!(cosmic.variant(), CosmicVariant::CosmicViolet);

        cosmic.cycle();
        assert_eq!(cosmic.variant(), CosmicVariant::CosmicDark);
    }

    #[test]
    fn test_cosmic_theme_set_variant() {
        let mut cosmic = CosmicTheme::default();
        cosmic.set_variant(CosmicVariant::CosmicViolet);
        assert_eq!(cosmic.variant(), CosmicVariant::CosmicViolet);
    }

    #[test]
    fn test_cosmic_theme_label() {
        let cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);
        assert!(cosmic.label().contains("Dark"));

        let cosmic = CosmicTheme::new(CosmicVariant::CosmicLight);
        assert!(cosmic.label().contains("Light"));

        let cosmic = CosmicTheme::new(CosmicVariant::CosmicViolet);
        assert!(cosmic.label().contains("Violet"));
    }

    #[test]
    fn test_cosmic_theme_is_dark() {
        let dark = CosmicTheme::new(CosmicVariant::CosmicDark);
        assert!(dark.is_dark());

        let light = CosmicTheme::new(CosmicVariant::CosmicLight);
        assert!(!light.is_dark());

        let violet = CosmicTheme::new(CosmicVariant::CosmicViolet);
        assert!(violet.is_dark());
    }

    #[test]
    fn test_cosmic_theme_as_theme() {
        let cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);
        let theme = cosmic.as_theme();

        // Verify theme has correct background from resolver
        let resolver = cosmic.resolver();
        assert_eq!(theme.background, resolver.bg_primary());
        assert_eq!(theme.text_primary, resolver.text_primary());
    }

    #[test]
    fn test_cosmic_theme_resolver_access() {
        let cosmic = CosmicTheme::new(CosmicVariant::CosmicLight);
        let resolver = cosmic.resolver();

        // Light theme should have light background
        assert!(!resolver.semantic().is_dark());
    }

    #[test]
    fn test_theme_mode_to_cosmic_variant() {
        assert_eq!(
            CosmicVariant::from(ThemeMode::Dark),
            CosmicVariant::CosmicDark
        );
        assert_eq!(
            CosmicVariant::from(ThemeMode::Light),
            CosmicVariant::CosmicLight
        );
        assert_eq!(
            CosmicVariant::from(ThemeMode::Solarized),
            CosmicVariant::CosmicDark
        );
    }

    #[test]
    fn test_cosmic_variant_to_theme_mode() {
        assert_eq!(ThemeMode::from(CosmicVariant::CosmicDark), ThemeMode::Dark);
        assert_eq!(
            ThemeMode::from(CosmicVariant::CosmicLight),
            ThemeMode::Light
        );
        assert_eq!(
            ThemeMode::from(CosmicVariant::CosmicViolet),
            ThemeMode::Dark
        );
    }

    #[test]
    fn test_theme_from_resolver_cosmic_dark() {
        let theme = Theme::cosmic_dark();

        // Should have Abyss Dark background
        assert_eq!(theme.background, Color::Rgb(8, 8, 14)); // Abyss
    }

    #[test]
    fn test_theme_from_resolver_cosmic_light() {
        let theme = Theme::cosmic_light();

        // Should have Slate-50 background
        assert_eq!(theme.background, Color::Rgb(248, 250, 252)); // #f8fafc
    }

    #[test]
    fn test_theme_from_resolver_cosmic_violet() {
        let theme = Theme::cosmic_violet();

        // Should have Violet-950 background
        assert_eq!(theme.background, Color::Rgb(46, 16, 101)); // #2e1065
    }

    #[test]
    fn test_theme_from_resolver_has_all_fields() {
        let resolver = TokenResolver::cosmic_dark();
        let theme = Theme::from_resolver(&resolver);

        // Check key fields are populated (not default/Reset)
        assert_ne!(theme.realm_shared, Color::Reset);
        assert_ne!(theme.realm_org, Color::Reset);
        assert_ne!(theme.trait_defined, Color::Reset);
        assert_ne!(theme.status_pending, Color::Reset);
        assert_ne!(theme.phase_launch, Color::Reset);
        assert_ne!(theme.mcp_describe, Color::Reset);
        assert_ne!(theme.border_normal, Color::Reset);
        assert_ne!(theme.scrollbar_thumb, Color::Reset);

        // Phase 1 expansion fields
        assert_ne!(theme.diagnostic_error, Color::Reset);
        assert_ne!(theme.diagnostic_warning, Color::Reset);
        assert_ne!(theme.diagnostic_hint, Color::Reset);
        assert_ne!(theme.popup_bg, Color::Reset);
        assert_ne!(theme.popup_border, Color::Reset);
        assert_ne!(theme.popup_selected, Color::Reset);
        assert_ne!(theme.verb_infer_glow, Color::Reset);
        assert_ne!(theme.verb_exec_glow, Color::Reset);
        assert_ne!(theme.verb_fetch_glow, Color::Reset);
        assert_ne!(theme.verb_invoke_glow, Color::Reset);
        assert_ne!(theme.verb_agent_glow, Color::Reset);
        assert_ne!(theme.tree_directory, Color::Reset);
        assert_ne!(theme.tree_file, Color::Reset);
        assert_ne!(theme.tree_hidden, Color::Reset);
        assert_ne!(theme.tree_ecosystem, Color::Reset);
        assert_ne!(theme.tree_ecosystem_glow, Color::Reset);
        assert_ne!(theme.tree_indent_guide, Color::Reset);
        assert_ne!(theme.syntax_keyword, Color::Reset);
        assert_ne!(theme.syntax_string, Color::Reset);
        assert_ne!(theme.syntax_number, Color::Reset);
        assert_ne!(theme.syntax_comment, Color::Reset);
        assert_ne!(theme.syntax_verb, Color::Reset);
        assert_ne!(theme.syntax_template, Color::Reset);
        assert_ne!(theme.syntax_mcp_server, Color::Reset);
        assert_ne!(theme.syntax_task_id, Color::Reset);
    }

    #[test]
    fn test_theme_cosmic_variants_differ() {
        let dark = Theme::cosmic_dark();
        let light = Theme::cosmic_light();
        let violet = Theme::cosmic_violet();

        // Backgrounds should all be different
        assert_ne!(dark.background, light.background);
        assert_ne!(dark.background, violet.background);
        assert_ne!(light.background, violet.background);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // CACHING TESTS
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_as_theme_caches_result() {
        let cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);

        // First call populates cache
        let theme1 = cosmic.as_theme();

        // Second call returns cached value (same result)
        let theme2 = cosmic.as_theme();

        assert_eq!(theme1.background, theme2.background);
        assert_eq!(theme1.text_primary, theme2.text_primary);
    }

    #[test]
    fn test_cycle_invalidates_cache() {
        let mut cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);

        // Get cached dark theme
        let dark_theme = cosmic.as_theme();
        assert_eq!(dark_theme.background, Color::Rgb(8, 8, 14)); // Abyss Dark

        // Cycle to light
        cosmic.cycle();

        // Cache should be invalidated, new theme returned
        let light_theme = cosmic.as_theme();
        assert_eq!(light_theme.background, Color::Rgb(248, 250, 252)); // Slate-50
        assert_ne!(dark_theme.background, light_theme.background);
    }

    #[test]
    fn test_set_variant_invalidates_cache() {
        let mut cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);

        // Get cached dark theme
        let dark_theme = cosmic.as_theme();

        // Set to violet
        cosmic.set_variant(CosmicVariant::CosmicViolet);

        // Cache should be invalidated
        let violet_theme = cosmic.as_theme();
        assert_eq!(violet_theme.background, Color::Rgb(46, 16, 101)); // Violet-950
        assert_ne!(dark_theme.background, violet_theme.background);
    }

    #[test]
    fn test_clone_gets_fresh_cache() {
        let cosmic1 = CosmicTheme::new(CosmicVariant::CosmicDark);

        // Populate cache on original
        let _ = cosmic1.as_theme();

        // Clone gets fresh cache (not shared)
        let cosmic2 = cosmic1.clone();

        // Both should return same values
        let theme1 = cosmic1.as_theme();
        let theme2 = cosmic2.as_theme();
        assert_eq!(theme1.background, theme2.background);
    }
}
