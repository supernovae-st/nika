// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A task or run state as a cell: the theme seam's own glyph column and role
//! (`nika_display::theme::Theme::glyph`), re-read here as data so a Ratatui
//! widget can paint it. A test pins every state, in both glyph columns, to
//! what the seam paints, so the renderer and the run frames never disagree
//! about what a state looks like. The glyph never stands alone: the caller
//! writes the state's words beside it.

use nika_display::state::TaskState;
use nika_display::theme::Role;

/// Every state the seam paints, in lifecycle order.
pub const STATES: [TaskState; 8] = [
    TaskState::Pending,
    TaskState::Running,
    TaskState::Ok,
    TaskState::Failed,
    TaskState::Retrying,
    TaskState::Skipped,
    TaskState::Cancelled,
    TaskState::Paused,
];

/// The glyph (still, never the spinner) and the role of `state`.
#[must_use]
pub const fn cell(state: TaskState, ascii: bool) -> (&'static str, Role) {
    let glyph = match (state, ascii) {
        (TaskState::Pending, false) => "○",
        (TaskState::Pending, true) => ".",
        (TaskState::Running, false) => "◐",
        (TaskState::Running, true) => ">",
        (TaskState::Ok, false) => "✔",
        (TaskState::Ok, true) => "ok",
        (TaskState::Failed, false) => "✖",
        (TaskState::Failed, true) => "X",
        (TaskState::Retrying, false) => "↻",
        (TaskState::Retrying, true) => "r",
        (TaskState::Skipped, false) => "↷",
        (TaskState::Skipped, true) => "~>",
        (TaskState::Cancelled, false) => "⊘",
        (TaskState::Cancelled, true) => "x",
        (TaskState::Paused, false) => "◇",
        (TaskState::Paused, true) => "?",
    };
    let role = match state {
        TaskState::Running => Role::Accent,
        TaskState::Ok => Role::Good,
        TaskState::Failed => Role::Bad,
        // A human's attention moves both forward: amber, never red, never dim.
        TaskState::Retrying | TaskState::Paused => Role::Warn,
        TaskState::Pending | TaskState::Skipped | TaskState::Cancelled => Role::Dim,
    };
    (glyph, role)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use nika_display::theme::Theme;

    /// The SGR parameter the theme paints `text` with.
    fn sgr(text: &str) -> String {
        text.strip_prefix("\x1b[")
            .and_then(|rest| rest.split('m').next())
            .expect("an SGR prefix")
            .to_owned()
    }

    #[test]
    fn every_state_matches_the_glyph_column_the_theme_paints() {
        for ascii in [false, true] {
            let theme = Theme::new(false, ascii, false);
            for state in STATES {
                let (glyph, _) = cell(state, ascii);
                let painted = theme.glyph(state, 0);
                assert_eq!(painted.trim_end(), glyph, "{state:?} ascii={ascii}");
            }
        }
    }

    #[test]
    fn every_state_wears_the_role_the_theme_paints() {
        let theme = Theme::new(true, false, false);
        for state in STATES {
            let (_, role) = cell(state, false);
            assert_eq!(
                sgr(&theme.glyph(state, 0)),
                sgr(&theme.paint(role, "x")),
                "{state:?}"
            );
        }
    }
}
