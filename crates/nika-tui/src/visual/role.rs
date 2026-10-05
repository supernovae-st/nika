// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The one way a widget gets colour: a semantic [`Role`] resolved at paint
//! time (crate spec §2). The roles are the engine's closed set,
//! [`nika_display::theme::Role`]; each maps to the Ratatui colour of the same
//! ANSI-16 slot the CLI frames paint, so the renderer and the run frames never
//! disagree about a meaning's hue, and both stay the user's terminal hues.

use nika_display::theme::Role;
use ratatui::style::{Color, Modifier, Style};

/// The style of `role`. A hue appears only when colour is on; dim and strong
/// are weights, not hues, and stay under `NO_COLOR` like the renderer's chrome.
#[must_use]
pub fn style(role: Role, color: bool) -> Style {
    let hue = |c: Color| {
        if color {
            Style::default().fg(c)
        } else {
            Style::default()
        }
    };
    match role {
        Role::Accent => hue(Color::Cyan),
        Role::Good => hue(Color::Green),
        Role::Bad => hue(Color::Red),
        Role::Warn => hue(Color::Yellow),
        Role::Dim => Style::default().add_modifier(Modifier::DIM),
        Role::Strong => Style::default().add_modifier(Modifier::BOLD),
        Role::VerbInfer => hue(Color::LightBlue),
        Role::VerbExec => hue(Color::LightYellow),
        Role::VerbInvoke => hue(Color::LightCyan),
        Role::VerbAgent => hue(Color::LightMagenta),
    }
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

/// The style of a verb chip: the verb's bright slot for the locked four, dim
/// for any other word, never a guessed identity.
#[must_use]
pub fn verb(word: &str, color: bool) -> Style {
    style(Role::for_verb(word).unwrap_or(Role::Dim), color)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_display::theme::Theme;

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

    /// The SGR parameter of a style this module builds (one colour or one weight).
    fn sgr(style: Style) -> u8 {
        match (style.fg, style.add_modifier) {
            (Some(Color::Red), _) => 31,
            (Some(Color::Green), _) => 32,
            (Some(Color::Yellow), _) => 33,
            (Some(Color::Cyan), _) => 36,
            (Some(Color::LightYellow), _) => 93,
            (Some(Color::LightBlue), _) => 94,
            (Some(Color::LightMagenta), _) => 95,
            (Some(Color::LightCyan), _) => 96,
            (None, m) if m == Modifier::BOLD => 1,
            (None, m) if m == Modifier::DIM => 2,
            other => panic!("unmapped style {other:?}"),
        }
    }

    /// The SGR parameter the CLI theme paints for `role`.
    fn painted(role: Role) -> u8 {
        let text = Theme::new(true, false, false).paint(role, "x");
        let code = text
            .strip_prefix("\x1b[")
            .and_then(|rest| rest.split('m').next())
            .expect("an SGR prefix");
        code.parse().expect("one numeric SGR parameter")
    }

    #[test]
    fn every_role_paints_the_slot_the_cli_theme_paints() {
        for role in ROLES {
            assert_eq!(sgr(style(role, true)), painted(role), "{role:?}");
        }
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
        // The verb band never collides with a verdict: bright cyan invoke is
        // not the accent's normal cyan.
        assert_ne!(verb("invoke", true), style(Role::Accent, true));
    }
}
