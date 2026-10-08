// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workspace palette through its public seam: every semantic role and
//! surface wears the hue of the approved terminal design, the verbs keep
//! identities apart from the accent, and `NO_COLOR` carries no hue at all.

use nika_display::theme::Role;
use nika_tui_view::visual::role::{style, surface, verb};
use ratatui::style::{Color, Modifier, Style};

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
fn semantic_roles_wear_the_approved_hues() {
    for (role, hue) in [
        // Violet accent and selection, cyan interaction, green success.
        (Role::Accent, Color::Rgb(182, 154, 255)),
        (Role::VerbInvoke, Color::Rgb(123, 219, 232)),
        (Role::Good, Color::Rgb(126, 208, 160)),
        (Role::Warn, Color::Rgb(233, 191, 126)),
        (Role::Bad, Color::Rgb(255, 150, 158)),
        (Role::Dim, Color::Rgb(156, 172, 197)),
        (Role::Strong, Color::Rgb(229, 233, 242)),
    ] {
        assert_eq!(style(role, true).fg, Some(hue), "{role:?}");
        assert_eq!(style(role, true).bg, None, "{role:?} paints no surface");
    }
    assert!(
        style(Role::Strong, true)
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert!(!style(Role::Dim, true).add_modifier.contains(Modifier::DIM));
}

#[test]
fn surfaces_are_the_approved_dark_ground_and_panel() {
    let ink = Color::Rgb(229, 233, 242);
    assert_eq!(
        surface(true, false),
        Style::default().fg(ink).bg(Color::Rgb(13, 17, 25))
    );
    assert_eq!(
        surface(true, true),
        Style::default().fg(ink).bg(Color::Rgb(16, 22, 33))
    );
}

#[test]
fn verbs_keep_identities_apart_from_the_violet_accent() {
    let accent = style(Role::Accent, true).fg;
    // The infer verb keeps its blue identity instead of borrowing the accent.
    assert_eq!(
        style(Role::VerbInfer, true).fg,
        Some(Color::Rgb(140, 177, 255))
    );
    for role in [Role::VerbInfer, Role::VerbInvoke, Role::VerbAgent] {
        assert_ne!(style(role, true).fg, accent, "{role:?}");
    }
    assert_eq!(style(Role::VerbExec, true).fg, style(Role::Warn, true).fg);
    assert_eq!(verb("invoke", true), style(Role::VerbInvoke, true));
    assert_eq!(verb("fetch", true), style(Role::Dim, true));
}

#[test]
fn without_colour_no_role_or_surface_carries_a_hue() {
    for role in ROLES {
        let plain = style(role, false);
        assert_eq!((plain.fg, plain.bg), (None, None), "{role:?}");
    }
    assert_eq!(surface(false, false), Style::default());
    assert_eq!(surface(false, true), Style::default());
    assert_eq!(style(Role::Dim, false).add_modifier, Modifier::DIM);
    assert_eq!(style(Role::Strong, false).add_modifier, Modifier::BOLD);
}
