// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Semantic roles resolved to the workspace's product palette, the hues of
//! the approved terminal design: an electric-blue accent and selection
//! (brand, focus, selection, primary action and interaction chrome), cyan
//! for the invoke verb, green for success, on subdued dark surfaces. The roles
//! still belong to `nika_display`, while these RGB hues belong only to the
//! TUI: the renderer and every viewer paint with this one mapping, and the
//! CLI keeps its own terminal-theme mapping. `NO_COLOR` carries no hue.
//!
//! The brand's own tones ([`Tone`]) are the shared design tokens' values:
//! the butterfly's ice and glow, and the exec hue of the wordmark's dot.
//! They alone follow the terminal's [`Depth`]: their own hue in 24-bit
//! colour, the nearest xterm-256 index, the nearest bright ANSI slot.

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
const SELECTION: Color = Color::Rgb(20, 45, 73);

/// `brand.color.mark.ice`: the butterfly's identity tone on dark surfaces.
const ICE: Color = Color::Rgb(0x9f, 0xd0, 0xff);
/// `brand.color.mark.glow`: light the reveal has just lit.
const GLOW: Color = Color::Rgb(0xcf, 0xe6, 0xff);
/// `verbs.exec.color`: the exec hue the wordmark's dot wears.
const EXEC: Color = Color::Rgb(0xff, 0x7a, 0x3c);

/// A tone of the brand lockup, the one place the brand's own hues appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Tone {
    /// The butterfly at rest.
    Ice,
    /// A part of the butterfly the reveal has just lit, before it settles.
    Glow,
    /// The dot that closes the wordmark.
    Dot,
}

/// How many hues a terminal cell can wear, as its caller read them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Depth {
    /// No hue: `NO_COLOR`, or colour refused.
    None,
    /// The 16 ANSI slots, whose hues the user's theme decides.
    Ansi16,
    /// The xterm 256-colour palette.
    Ansi256,
    /// 24-bit colour.
    Rgb,
}

impl Depth {
    /// The depth a caller's readings prove: none without `color`, 24-bit
    /// where `truecolor` says `COLORTERM` read `truecolor` or `24bit`, the
    /// 256 colours a `term` such as `xterm-256color` names, else the 16 slots.
    #[must_use]
    pub fn of(color: bool, truecolor: bool, term: Option<&str>) -> Self {
        if !color {
            Self::None
        } else if truecolor {
            Self::Rgb
        } else if term.is_some_and(|term| term.contains("256color")) {
            Self::Ansi256
        } else {
            Self::Ansi16
        }
    }

    /// The depth the workspace palette itself assumes: 24-bit wherever
    /// colour is allowed, as every role above.
    #[must_use]
    pub const fn of_palette(color: bool) -> Self {
        if color { Self::Rgb } else { Self::None }
    }
}

/// The style of a brand `tone` at `depth`: the token's own hue in 24-bit
/// colour; its nearest xterm-256 index (least RGB distance over the cube and
/// the grey ramp); in the 16 slots, bright cyan for the butterfly at rest
/// (the nearest bright blue or cyan), bright white for new light and the exec
/// verb's own bright yellow for the dot; no hue at all without colour.
#[must_use]
pub fn tone(tone: Tone, depth: Depth) -> Style {
    let hue = match (depth, tone) {
        (Depth::None, _) => return Style::default(),
        (Depth::Rgb, Tone::Ice) => ICE,
        (Depth::Rgb, Tone::Glow) => GLOW,
        (Depth::Rgb, Tone::Dot) => EXEC,
        (Depth::Ansi256, Tone::Ice) => Color::Indexed(153),
        (Depth::Ansi256, Tone::Glow) => Color::Indexed(189),
        (Depth::Ansi256, Tone::Dot) => Color::Indexed(209),
        (Depth::Ansi16, Tone::Ice) => Color::LightCyan,
        (Depth::Ansi16, Tone::Glow) => Color::White,
        (Depth::Ansi16, Tone::Dot) => Color::LightYellow,
    };
    Style::default().fg(hue)
}

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
        Role::Accent => Color::Rgb(76, 163, 255),
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
            (Role::Accent, Color::Rgb(76, 163, 255)),
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

    /// The value `key: "#rrggbb"` takes in the vendored design tokens, read
    /// after the line `section` (the shared vocabulary is spec-first: a hue
    /// changes there and this table follows).
    fn token(section: &str, key: &str) -> Color {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../nika-pack/pack/design-tokens.yaml");
        let tokens = std::fs::read_to_string(&path).expect("the vendored design tokens");
        let hex = tokens
            .split_once(section)
            .and_then(|(_, rest)| {
                rest.lines()
                    .find_map(|line| line.trim().strip_prefix(&format!("{key}: \"#")))
            })
            .and_then(|rest| rest.get(..6))
            .unwrap_or_else(|| panic!("{key} under {section:?} in {}", path.display()));
        let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex");
        Color::Rgb(channel(0), channel(2), channel(4))
    }

    #[test]
    fn brand_tones_are_the_shared_design_tokens() {
        assert_eq!(token("    mark:\n", "ice"), ICE);
        assert_eq!(token("    mark:\n", "glow"), GLOW);
        assert_eq!(token("  exec:\n", "color"), EXEC);
        assert_eq!(tone(Tone::Ice, Depth::Rgb), Style::default().fg(ICE));
        assert_eq!(tone(Tone::Glow, Depth::Rgb), Style::default().fg(GLOW));
        assert_eq!(tone(Tone::Dot, Depth::Rgb), Style::default().fg(EXEC));
    }

    /// The xterm-256 colour of `index` (16 to 255: the 6×6×6 cube, then the
    /// grey ramp).
    fn xterm(index: u8) -> (i32, i32, i32) {
        const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
        let i = usize::from(index);
        if i >= 232 {
            let grey = 8 + 10 * (i32::from(index) - 232);
            return (grey, grey, grey);
        }
        let cube = i - 16;
        (LEVELS[cube / 36], LEVELS[cube / 6 % 6], LEVELS[cube % 6])
    }

    #[test]
    fn every_smaller_palette_takes_the_nearest_slot() {
        for brand in [Tone::Ice, Tone::Glow, Tone::Dot] {
            let Some(Color::Rgb(red, green, blue)) = tone(brand, Depth::Rgb).fg else {
                panic!("{brand:?} has no 24-bit hue");
            };
            let wanted = [red, green, blue].map(i32::from);
            let distance = |index: u8| {
                let (cube_red, cube_green, cube_blue) = xterm(index);
                [cube_red, cube_green, cube_blue]
                    .iter()
                    .zip(wanted)
                    .map(|(slot, want)| (slot - want).pow(2))
                    .sum::<i32>()
            };
            let nearest = (16..=255).min_by_key(|index| distance(*index));
            assert_eq!(
                tone(brand, Depth::Ansi256).fg,
                nearest.map(Color::Indexed),
                "{brand:?}"
            );
        }
        // The 16 slots are the user's theme: the mark keeps a bright blue or
        // cyan, new light the brightest slot, the dot the exec verb's own.
        assert_eq!(tone(Tone::Ice, Depth::Ansi16).fg, Some(Color::LightCyan));
        assert_eq!(tone(Tone::Glow, Depth::Ansi16).fg, Some(Color::White));
        assert_eq!(tone(Tone::Dot, Depth::Ansi16).fg, Some(Color::LightYellow));
        let exec = nika_display::theme::Theme::new(true, false, false).paint(Role::VerbExec, "");
        assert!(exec.starts_with("\x1b[93m"), "{exec:?}");
    }

    #[test]
    fn without_colour_no_tone_carries_a_hue() {
        for brand in [Tone::Ice, Tone::Glow, Tone::Dot] {
            assert_eq!(tone(brand, Depth::None), Style::default(), "{brand:?}");
        }
    }

    #[test]
    fn the_depth_follows_what_its_caller_read() {
        let xterm = Some("xterm-256color");
        assert_eq!(Depth::of(false, true, xterm), Depth::None);
        assert_eq!(Depth::of(true, true, xterm), Depth::Rgb);
        assert_eq!(Depth::of(true, true, None), Depth::Rgb);
        assert_eq!(Depth::of(true, false, xterm), Depth::Ansi256);
        assert_eq!(
            Depth::of(true, false, Some("tmux-256color")),
            Depth::Ansi256
        );
        assert_eq!(Depth::of(true, false, Some("xterm")), Depth::Ansi16);
        assert_eq!(Depth::of(true, false, None), Depth::Ansi16);
        assert_eq!(Depth::of_palette(true), Depth::Rgb);
        assert_eq!(Depth::of_palette(false), Depth::None);
    }

    #[test]
    fn verbs_keep_their_bright_band_and_unknown_words_stay_dim() {
        assert_eq!(verb("infer", true), style(Role::VerbInfer, true));
        assert_eq!(verb("exec", true), style(Role::VerbExec, true));
        assert_eq!(verb("invoke", true), style(Role::VerbInvoke, true));
        assert_eq!(verb("agent", true), style(Role::VerbAgent, true));
        assert_eq!(verb("fetch", true), style(Role::Dim, true));
        // Invoke keeps its cyan identity and infer its blue beside the electric-blue accent.
        assert_ne!(verb("invoke", true), style(Role::Accent, true));
        assert_ne!(verb("infer", true), style(Role::Accent, true));
    }
}
