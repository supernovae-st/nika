// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Integration tests for CosmicTheme → Theme bridge
//!
//! These tests verify the complete rendering pipeline from CosmicTheme
//! through to widget rendering, ensuring themes are consistent across
//! all TUI views.

#![cfg(feature = "tui")]

use nika::tui::{CosmicTheme, CosmicVariant, Theme, TokenResolver};
use ratatui::style::Color;

// ═══════════════════════════════════════════════════════════════════════════════
// BRIDGE INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_cosmic_theme_produces_valid_theme() {
    let cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);
    let theme = cosmic.as_theme();

    // All key fields should be populated
    assert_ne!(theme.background, Color::Reset);
    assert_ne!(theme.text_primary, Color::Reset);
    assert_ne!(theme.border_normal, Color::Reset);
    assert_ne!(theme.highlight, Color::Reset);
}

#[test]
fn test_resolver_and_theme_have_consistent_colors() {
    let cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);
    let theme = cosmic.as_theme();
    let resolver = cosmic.resolver();

    // Background colors should match
    assert_eq!(theme.background, resolver.bg_primary());
    assert_eq!(theme.text_primary, resolver.text_primary());
    assert_eq!(theme.border_normal, resolver.semantic().border_default);
    assert_eq!(theme.highlight, resolver.semantic().accent_primary);
}

#[test]
fn test_all_variants_produce_distinct_themes() {
    let dark = CosmicTheme::new(CosmicVariant::CosmicDark).as_theme();
    let light = CosmicTheme::new(CosmicVariant::CosmicLight).as_theme();
    let violet = CosmicTheme::new(CosmicVariant::CosmicViolet).as_theme();

    // Backgrounds should all be different
    assert_ne!(dark.background, light.background);
    assert_ne!(dark.background, violet.background);
    assert_ne!(light.background, violet.background);

    // Primary text should differ (inverted for light)
    assert_ne!(dark.text_primary, light.text_primary);
}

#[test]
fn test_theme_cycling_produces_correct_sequence() {
    let mut cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);

    // Dark → Light → Violet → Dark
    let dark_bg = cosmic.as_theme().background;
    assert_eq!(dark_bg, Color::Rgb(15, 23, 42)); // Slate-900

    cosmic.cycle();
    let light_bg = cosmic.as_theme().background;
    assert_eq!(light_bg, Color::Rgb(248, 250, 252)); // Slate-50

    cosmic.cycle();
    let violet_bg = cosmic.as_theme().background;
    assert_eq!(violet_bg, Color::Rgb(46, 16, 101)); // Violet-950

    cosmic.cycle();
    let back_to_dark = cosmic.as_theme().background;
    assert_eq!(back_to_dark, dark_bg); // Back to dark
}

// ═══════════════════════════════════════════════════════════════════════════════
// TAILWIND COLOR ACCURACY TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_cosmic_dark_uses_correct_tailwind_slate() {
    let resolver = TokenResolver::cosmic_dark();

    // Slate-900 background
    assert_eq!(resolver.bg_primary(), Color::Rgb(15, 23, 42));
    // Slate-800 secondary bg
    assert_eq!(resolver.semantic().bg_secondary, Color::Rgb(30, 41, 59));
    // Slate-50 text (bright white for dark mode)
    assert_eq!(resolver.text_primary(), Color::Rgb(248, 250, 252));
}

#[test]
fn test_cosmic_light_uses_correct_tailwind_slate() {
    let resolver = TokenResolver::cosmic_light();

    // Slate-50 background
    assert_eq!(resolver.bg_primary(), Color::Rgb(248, 250, 252));
    // Slate-100 secondary bg
    assert_eq!(resolver.semantic().bg_secondary, Color::Rgb(241, 245, 249));
    // Slate-900 text
    assert_eq!(resolver.text_primary(), Color::Rgb(15, 23, 42));
}

#[test]
fn test_cosmic_violet_uses_correct_tailwind_violet() {
    let resolver = TokenResolver::cosmic_violet();

    // Violet-950 background
    assert_eq!(resolver.bg_primary(), Color::Rgb(46, 16, 101));
    // Violet-900 secondary bg
    assert_eq!(resolver.semantic().bg_secondary, Color::Rgb(76, 29, 149));
}

// ═══════════════════════════════════════════════════════════════════════════════
// LEGACY API COMPATIBILITY TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_theme_default_returns_cosmic_dark() {
    let default_theme = Theme::default();
    let cosmic_dark = Theme::cosmic_dark();

    assert_eq!(default_theme.background, cosmic_dark.background);
    assert_eq!(default_theme.text_primary, cosmic_dark.text_primary);
}

#[test]
fn test_theme_dark_returns_cosmic_dark() {
    let dark = Theme::dark();
    let cosmic_dark = Theme::cosmic_dark();

    assert_eq!(dark.background, cosmic_dark.background);
}

#[test]
fn test_theme_light_returns_cosmic_light() {
    let light = Theme::light();
    let cosmic_light = Theme::cosmic_light();

    assert_eq!(light.background, cosmic_light.background);
}

#[test]
fn test_theme_solarized_returns_cosmic_dark() {
    let solarized = Theme::solarized();
    let cosmic_dark = Theme::cosmic_dark();

    // Solarized maps to cosmic dark for now
    assert_eq!(solarized.background, cosmic_dark.background);
}

// ═══════════════════════════════════════════════════════════════════════════════
// SEMANTIC COLOR MAPPING TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_theme_maps_all_status_colors() {
    let theme = Theme::cosmic_dark();

    // All status colors should be distinct
    assert_ne!(theme.status_pending, theme.status_running);
    assert_ne!(theme.status_running, theme.status_success);
    assert_ne!(theme.status_success, theme.status_failed);
    assert_ne!(theme.status_failed, theme.status_paused);
}

#[test]
fn test_theme_maps_all_trait_colors() {
    let theme = Theme::cosmic_dark();

    // All trait colors should be set
    assert_ne!(theme.trait_defined, Color::Reset);
    assert_ne!(theme.trait_authored, Color::Reset);
    assert_ne!(theme.trait_imported, Color::Reset);
    assert_ne!(theme.trait_generated, Color::Reset);
    assert_ne!(theme.trait_retrieved, Color::Reset);
}

#[test]
fn test_theme_maps_all_phase_colors() {
    let theme = Theme::cosmic_dark();

    // Phase colors should be distinct
    assert_ne!(theme.phase_launch, theme.phase_orbital);
    assert_ne!(theme.phase_orbital, theme.phase_rendezvous);
}

#[test]
fn test_theme_maps_scrollbar_colors() {
    let theme = Theme::cosmic_dark();

    assert_ne!(theme.scrollbar_thumb, Color::Reset);
    assert_ne!(theme.scrollbar_track, Color::Reset);
    assert_ne!(theme.scrollbar_arrows, Color::Reset);
}

// ═══════════════════════════════════════════════════════════════════════════════
// DARK/LIGHT MODE DETECTION TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_cosmic_dark_is_dark_mode() {
    let cosmic = CosmicTheme::new(CosmicVariant::CosmicDark);
    assert!(cosmic.is_dark());
}

#[test]
fn test_cosmic_light_is_not_dark_mode() {
    let cosmic = CosmicTheme::new(CosmicVariant::CosmicLight);
    assert!(!cosmic.is_dark());
}

#[test]
fn test_cosmic_violet_is_dark_mode() {
    let cosmic = CosmicTheme::new(CosmicVariant::CosmicViolet);
    assert!(cosmic.is_dark());
}

// ═══════════════════════════════════════════════════════════════════════════════
// WCAG CONTRAST RATIO TESTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Calculate relative luminance for a color (WCAG formula)
/// https://www.w3.org/WAI/GL/wiki/Relative_luminance
fn relative_luminance(color: Color) -> f64 {
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => return 0.0, // Non-RGB colors default to black
    };

    let r = srgb_to_linear(r as f64 / 255.0);
    let g = srgb_to_linear(g as f64 / 255.0);
    let b = srgb_to_linear(b as f64 / 255.0);

    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Calculate contrast ratio between two colors (WCAG formula)
/// https://www.w3.org/WAI/GL/wiki/Contrast_ratio
fn contrast_ratio(foreground: Color, background: Color) -> f64 {
    let l1 = relative_luminance(foreground);
    let l2 = relative_luminance(background);
    let lighter = l1.max(l2);
    let darker = l1.min(l2);
    (lighter + 0.05) / (darker + 0.05)
}

const WCAG_AA_NORMAL: f64 = 4.5;
const WCAG_AA_LARGE: f64 = 3.0;

#[test]
fn test_cosmic_dark_text_contrast_meets_wcag_aa() {
    let resolver = TokenResolver::cosmic_dark();
    let bg = resolver.bg_primary();
    let text = resolver.text_primary();

    let ratio = contrast_ratio(text, bg);
    assert!(
        ratio >= WCAG_AA_NORMAL,
        "Cosmic Dark text contrast {:.2}:1 should meet WCAG AA ({:.1}:1)",
        ratio,
        WCAG_AA_NORMAL
    );
}

#[test]
fn test_cosmic_light_text_contrast_meets_wcag_aa() {
    let resolver = TokenResolver::cosmic_light();
    let bg = resolver.bg_primary();
    let text = resolver.text_primary();

    let ratio = contrast_ratio(text, bg);
    assert!(
        ratio >= WCAG_AA_NORMAL,
        "Cosmic Light text contrast {:.2}:1 should meet WCAG AA ({:.1}:1)",
        ratio,
        WCAG_AA_NORMAL
    );
}

#[test]
fn test_cosmic_violet_text_contrast_meets_wcag_aa() {
    let resolver = TokenResolver::cosmic_violet();
    let bg = resolver.bg_primary();
    let text = resolver.text_primary();

    let ratio = contrast_ratio(text, bg);
    assert!(
        ratio >= WCAG_AA_NORMAL,
        "Cosmic Violet text contrast {:.2}:1 should meet WCAG AA ({:.1}:1)",
        ratio,
        WCAG_AA_NORMAL
    );
}

#[test]
fn test_cosmic_dark_secondary_text_meets_wcag_aa_large() {
    let resolver = TokenResolver::cosmic_dark();
    let bg = resolver.bg_primary();
    let text_secondary = resolver.semantic().text_secondary;

    let ratio = contrast_ratio(text_secondary, bg);
    assert!(
        ratio >= WCAG_AA_LARGE,
        "Cosmic Dark secondary text contrast {:.2}:1 should meet WCAG AA large ({:.1}:1)",
        ratio,
        WCAG_AA_LARGE
    );
}

#[test]
fn test_cosmic_dark_border_visible() {
    let resolver = TokenResolver::cosmic_dark();
    let bg = resolver.bg_primary();
    let border = resolver.semantic().border_default;

    let ratio = contrast_ratio(border, bg);
    // Borders need less contrast but should be visible
    assert!(
        ratio >= 1.5,
        "Cosmic Dark border contrast {:.2}:1 should be visible (≥1.5:1)",
        ratio
    );
}

#[test]
fn test_cosmic_dark_status_colors_visible() {
    let theme = Theme::cosmic_dark();
    let bg = theme.background;

    // All status colors should have sufficient contrast
    let status_colors = [
        ("success", theme.status_success),
        ("failed", theme.status_failed),
        ("running", theme.status_running),
        ("paused", theme.status_paused),
    ];

    for (name, color) in status_colors {
        let ratio = contrast_ratio(color, bg);
        assert!(
            ratio >= WCAG_AA_LARGE,
            "Status {} contrast {:.2}:1 should meet WCAG AA large ({:.1}:1)",
            name,
            ratio,
            WCAG_AA_LARGE
        );
    }
}

#[test]
fn test_cosmic_light_status_colors_visible() {
    let theme = Theme::cosmic_light();
    let bg = theme.background;

    let status_colors = [
        ("success", theme.status_success),
        ("failed", theme.status_failed),
        ("running", theme.status_running),
    ];

    for (name, color) in status_colors {
        let ratio = contrast_ratio(color, bg);
        assert!(
            ratio >= WCAG_AA_LARGE,
            "Light status {} contrast {:.2}:1 should meet WCAG AA large ({:.1}:1)",
            name,
            ratio,
            WCAG_AA_LARGE
        );
    }
}
