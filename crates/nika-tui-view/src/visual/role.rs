// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Semantic roles resolved to the workspace's product palette, the hues of
//! the approved terminal design: a violet accent and selection, cyan for
//! interaction, green for success, on subdued dark surfaces. The roles
//! still belong to `nika_display`, while these RGB hues belong only to the
//! TUI: the renderer and every viewer paint with this one mapping, and the
//! CLI keeps its own terminal-theme mapping. `NO_COLOR` carries no hue.

use nika_display::theme::Role;
use ratatui::style::{Color, Modifier, Style};

/// Primary text on every surface.
const INK: Color = Color::Rgb(229, 233, 242);
/// The workspace ground.
const GROUND: Color = Color::Rgb(13, 17, 25);
/// A raised panel on the ground.
const PANEL: Color = Color::Rgb(16, 22, 33);
/// The subdued frame of an object at rest.
const BORDER: Color = Color::Rgb(45, 59, 82);
/// The fill of the selected or active object.
const SELECTION: Color = Color::Rgb(37, 39, 62);

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
        Role::Accent => Color::Rgb(182, 154, 255),
        Role::VerbInfer => Color::Rgb(140, 177, 255),
        Role::Good => Color::Rgb(126, 208, 160),
        Role::Bad => Color::Rgb(255, 150, 158),
        Role::Warn | Role::VerbExec => Color::Rgb(233, 191, 126),
        Role::Dim => Color::Rgb(156, 172, 197),
        Role::Strong => INK,
        Role::VerbInvoke => Color::Rgb(123, 219, 232),
        Role::VerbAgent => Color::Rgb(194, 163, 242),
    };
    plain.fg(hue)
}

/// Neutral workspace surfaces from the product palette. Semantic hues still
/// come from [`style`]; without colour the terminal supplies both foreground
/// and background.
#[must_use]
pub fn surface(color: bool, raised: bool) -> Style {
    if color {
        Style::default()
            .fg(INK)
            .bg(if raised { PANEL } else { GROUND })
    } else {
        Style::default()
    }
}

/// The frame of an object at rest: the subdued border hue, or a dim weight
/// without colour.
#[must_use]
pub fn border(color: bool) -> Style {
    if color {
        Style::default().fg(BORDER)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    }
}

/// The fill of the selected or active object. Without colour there is no
/// fill: the caller's words and weights say which object it is.
#[must_use]
pub fn selection(color: bool) -> Style {
    if color {
        Style::default().bg(SELECTION)
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
            (Role::Accent, Color::Rgb(182, 154, 255)),
            (Role::Good, Color::Rgb(126, 208, 160)),
            (Role::Bad, Color::Rgb(255, 150, 158)),
            (Role::Warn, Color::Rgb(233, 191, 126)),
            (Role::Dim, Color::Rgb(156, 172, 197)),
            (Role::Strong, Color::Rgb(229, 233, 242)),
            (Role::VerbInfer, Color::Rgb(140, 177, 255)),
            (Role::VerbExec, Color::Rgb(233, 191, 126)),
            (Role::VerbInvoke, Color::Rgb(123, 219, 232)),
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
    fn frames_and_the_selection_fill_stay_subdued_and_vanish_without_colour() {
        assert_eq!(border(true), Style::default().fg(BORDER));
        assert_eq!(border(false), Style::default().add_modifier(Modifier::DIM));
        assert_eq!(selection(true), Style::default().bg(SELECTION));
        assert_eq!(selection(false), Style::default());
        // The fill lifts the selected object off the ground, below its frame.
        let light = |hue: Color| match hue {
            Color::Rgb(r, g, b) => u32::from(r) + u32::from(g) + u32::from(b),
            other => panic!("not an RGB hue: {other:?}"),
        };
        assert!(light(GROUND) < light(PANEL) && light(PANEL) < light(SELECTION));
        assert!(light(SELECTION) < light(BORDER));
    }

    #[test]
    fn verbs_keep_their_bright_band_and_unknown_words_stay_dim() {
        assert_eq!(verb("infer", true), style(Role::VerbInfer, true));
        assert_eq!(verb("exec", true), style(Role::VerbExec, true));
        assert_eq!(verb("invoke", true), style(Role::VerbInvoke, true));
        assert_eq!(verb("agent", true), style(Role::VerbAgent, true));
        assert_eq!(verb("fetch", true), style(Role::Dim, true));
        // Invoke keeps its cyan identity and infer its blue beside the violet accent.
        assert_ne!(verb("invoke", true), style(Role::Accent, true));
        assert_ne!(verb("infer", true), style(Role::Accent, true));
    }
}
