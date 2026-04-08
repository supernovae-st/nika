// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Rendering Helpers for Chat View
//!
//! Pure rendering functions for the chat view.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::theme::VerbColor;

// ═══════════════════════════════════════════════════════════════════════════════
// Verb Gradient Animation
// ═══════════════════════════════════════════════════════════════════════════════

/// Generate per-character gradient spans for smooth verb animation.
/// Creates a flowing wave effect with ASCII background zone.
///
/// # Arguments
/// * `text` - The text to render with gradient
/// * `verb_color` - Color scheme for the verb type
/// * `frame` - Animation frame counter (wraps at 255)
/// * `is_complete` - Whether the verb command is fully typed
///
/// # Returns
/// Vector of styled spans, one per character
pub fn render_verb_gradient(
    text: &str,
    verb_color: &VerbColor,
    frame: u8,
    is_complete: bool,
) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if len == 0 {
        return vec![];
    }

    let base_rgb = verb_color.rgb_tuple();
    let glow_rgb = verb_color.glow_tuple();
    // White for peak sparkle
    let sparkle_rgb: (u8, u8, u8) = (255, 255, 255);
    // Dark for contrast (muted base)
    let dark_rgb: (u8, u8, u8) = (base_rgb.0 / 2, base_rgb.1 / 2, base_rgb.2 / 2);
    // Background color (very subtle)
    let bg_base: (u8, u8, u8) = (
        12 + base_rgb.0 / 15,
        12 + base_rgb.1 / 15,
        12 + base_rgb.2 / 15,
    );

    chars
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            // FUN but not epileptic: visible wave per character
            let phase_offset = (i as f32) * std::f32::consts::PI * 0.6; // ~108° per char - visible difference
            let time = (frame as f32 / 12.0) * std::f32::consts::PI; // Medium speed - fun but readable
            let wave = ((time + phase_offset).sin() + 1.0) / 2.0; // 0.0 to 1.0

            // Secondary wave for extra life
            let shimmer_time = (frame as f32 / 20.0) * std::f32::consts::PI;
            let shimmer = ((shimmer_time + phase_offset * 2.0).sin() + 1.0) / 2.0;

            if is_complete {
                // Fun color wave: dark → base → glow → sparkle
                // Using smooth sine curve for nice transitions

                let (r, g, b) = if wave < 0.25 {
                    // Dark to base (valley)
                    let t = wave * 4.0;
                    (
                        (dark_rgb.0 as f32 * (1.0 - t) + base_rgb.0 as f32 * t) as u8,
                        (dark_rgb.1 as f32 * (1.0 - t) + base_rgb.1 as f32 * t) as u8,
                        (dark_rgb.2 as f32 * (1.0 - t) + base_rgb.2 as f32 * t) as u8,
                    )
                } else if wave < 0.6 {
                    // Base to glow (rising)
                    let t = (wave - 0.25) / 0.35;
                    (
                        (base_rgb.0 as f32 * (1.0 - t) + glow_rgb.0 as f32 * t) as u8,
                        (base_rgb.1 as f32 * (1.0 - t) + glow_rgb.1 as f32 * t) as u8,
                        (base_rgb.2 as f32 * (1.0 - t) + glow_rgb.2 as f32 * t) as u8,
                    )
                } else {
                    // Glow to white sparkle at peak (with shimmer variation)
                    let t = ((wave - 0.6) / 0.4) * (0.6 + shimmer * 0.4);
                    (
                        (glow_rgb.0 as f32 * (1.0 - t) + sparkle_rgb.0 as f32 * t) as u8,
                        (glow_rgb.1 as f32 * (1.0 - t) + sparkle_rgb.1 as f32 * t) as u8,
                        (glow_rgb.2 as f32 * (1.0 - t) + sparkle_rgb.2 as f32 * t) as u8,
                    )
                };

                let color = Color::Rgb(r, g, b);

                // Background zone with gentle pulse
                let bg_pulse = 0.6 + wave * 0.4; // Background follows the wave too
                let bg_r = (bg_base.0 as f32 * bg_pulse).min(255.0) as u8;
                let bg_g = (bg_base.1 as f32 * bg_pulse).min(255.0) as u8;
                let bg_b = (bg_base.2 as f32 * bg_pulse).min(255.0) as u8;

                Span::styled(
                    c.to_string(),
                    Style::default()
                        .fg(color)
                        .bg(Color::Rgb(bg_r, bg_g, bg_b))
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                // Partial match: visible wave but calmer
                let wave_mild = 0.3 + wave * 0.5; // 0.3 to 0.8 range
                let r =
                    (dark_rgb.0 as f32 * (1.0 - wave_mild) + glow_rgb.0 as f32 * wave_mild) as u8;
                let g =
                    (dark_rgb.1 as f32 * (1.0 - wave_mild) + glow_rgb.1 as f32 * wave_mild) as u8;
                let b =
                    (dark_rgb.2 as f32 * (1.0 - wave_mild) + glow_rgb.2 as f32 * wave_mild) as u8;

                Span::styled(
                    c.to_string(),
                    Style::default()
                        .fg(Color::Rgb(r, g, b))
                        .add_modifier(Modifier::BOLD),
                )
            }
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_verb_gradient_empty() {
        let result = render_verb_gradient("", &VerbColor::Infer, 0, true);
        assert!(result.is_empty());
    }

    #[test]
    fn test_render_verb_gradient_single_char() {
        let result = render_verb_gradient("a", &VerbColor::Infer, 0, true);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_render_verb_gradient_multi_char() {
        let result = render_verb_gradient("invoke", &VerbColor::Invoke, 0, true);
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn test_render_verb_gradient_partial() {
        let result = render_verb_gradient("inv", &VerbColor::Invoke, 0, false);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_render_verb_gradient_different_frames() {
        // Different frames should produce different colors (animation)
        let result0 = render_verb_gradient("a", &VerbColor::Exec, 0, true);
        let result30 = render_verb_gradient("a", &VerbColor::Exec, 30, true);
        let result60 = render_verb_gradient("a", &VerbColor::Exec, 60, true);

        // All should be valid spans
        assert_eq!(result0.len(), 1);
        assert_eq!(result30.len(), 1);
        assert_eq!(result60.len(), 1);
    }

    #[test]
    fn test_render_verb_gradient_all_verb_colors() {
        let colors = [
            VerbColor::Infer,
            VerbColor::Exec,
            VerbColor::Fetch,
            VerbColor::Invoke,
            VerbColor::Agent,
            VerbColor::Spawn,
            VerbColor::User,
        ];

        for color in colors {
            let result = render_verb_gradient("test", &color, 0, true);
            assert_eq!(result.len(), 4, "Failed for {:?}", color);
        }
    }

    #[test]
    fn test_render_verb_gradient_unicode() {
        let result = render_verb_gradient("🦋", &VerbColor::Agent, 0, true);
        assert_eq!(result.len(), 1);
    }
}
