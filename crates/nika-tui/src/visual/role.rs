// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Semantic roles resolved to the workspace's product palette. The roles
//! still belong to `nika_display`, while these RGB hues belong only to the
//! TUI and its viewer member; the CLI keeps its own terminal-theme mapping.
//! The two members pin the same palette values. `NO_COLOR` carries no hue.

use nika_display::theme::Role;
use ratatui::style::{Color, Modifier, Style};

/// The style of `role`. The secondary RGB text stays legible on dark
/// surfaces. Without colour, dim and strong retain only their text weights.
#[must_use]
pub fn style(role: Role, color: bool) -> Style {
    let plain = match role {
        Role::Dim if !color => Style::default().add_modifier(Modifier::DIM),
        Role::Strong => Style::default().add_modifier(Modifier::BOLD),
        _ => Style::default(),
    };
    if !color {
        return plain;
    }
    let hue = match role {
        Role::Accent | Role::VerbInfer => Color::Rgb(140, 177, 255),
        Role::Good => Color::Rgb(123, 210, 167),
        Role::Bad => Color::Rgb(255, 145, 162),
        Role::Warn | Role::VerbExec => Color::Rgb(242, 193, 125),
        Role::Dim => Color::Rgb(148, 165, 191),
        Role::Strong => Color::Rgb(224, 233, 247),
        Role::VerbInvoke => Color::Rgb(106, 216, 226),
        Role::VerbAgent => Color::Rgb(194, 163, 242),
    };
    plain.fg(hue)
}

/// Neutral workspace surfaces from the product palette. Semantic hues still
/// come from [`style`]; without colour the terminal supplies both foreground
/// and background.
pub(crate) fn surface(color: bool, raised: bool) -> Style {
    if color {
        Style::default()
            .fg(Color::Rgb(224, 233, 247))
            .bg(if raised {
                Color::Rgb(23, 33, 53)
            } else {
                Color::Rgb(12, 17, 28)
            })
    } else {
        Style::default()
    }
}

/// The style of a verb chip: the verb's identity hue for the locked four, secondary
/// for any other word, never a guessed identity.
#[must_use]
pub fn verb(word: &str, color: bool) -> Style {
    style(Role::for_verb(word).unwrap_or(Role::Dim), color)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const ROLES: [Role; 10] = [
        Role::Accent,
        Role::Good,
        Role::Bad,
        Role::Warn,
        Role::Dim,
        Role::Strong,
        Role::VerbInfer,
        Role::VerbExec,
        Role::VerbInvoke,
        Role::VerbAgent,
    ];

    #[test]
    fn semantic_roles_use_the_product_palette_with_readable_secondary_text() {
        for (role, color) in [
            (Role::Accent, Color::Rgb(140, 177, 255)),
            (Role::Good, Color::Rgb(123, 210, 167)),
            (Role::Bad, Color::Rgb(255, 145, 162)),
            (Role::Warn, Color::Rgb(242, 193, 125)),
            (Role::Dim, Color::Rgb(148, 165, 191)),
            (Role::Strong, Color::Rgb(224, 233, 247)),
            (Role::VerbInfer, Color::Rgb(140, 177, 255)),
            (Role::VerbExec, Color::Rgb(242, 193, 125)),
            (Role::VerbInvoke, Color::Rgb(106, 216, 226)),
            (Role::VerbAgent, Color::Rgb(194, 163, 242)),
        ] {
            assert_eq!(style(role, true).fg, Some(color), "{role:?}");
        }
        assert!(!style(Role::Dim, true).add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn without_colour_no_role_carries_a_hue_but_weights_remain() {
        for role in ROLES {
            let plain = style(role, false);
            assert_eq!(plain.fg, None, "{role:?}");
            assert_eq!(plain.bg, None, "{role:?}");
        }
        assert_eq!(style(Role::Dim, false).add_modifier, Modifier::DIM);
        assert_eq!(style(Role::Strong, false).add_modifier, Modifier::BOLD);
    }

    #[test]
    fn verbs_keep_their_bright_band_and_unknown_words_stay_dim() {
        assert_eq!(verb("infer", true), style(Role::VerbInfer, true));
        assert_eq!(verb("exec", true), style(Role::VerbExec, true));
        assert_eq!(verb("invoke", true), style(Role::VerbInvoke, true));
        assert_eq!(verb("agent", true), style(Role::VerbAgent, true));
        assert_eq!(verb("fetch", true), style(Role::Dim, true));
        // Invoke keeps its cyan identity beside the blue activity accent.
        assert_ne!(verb("invoke", true), style(Role::Accent, true));
    }
}
