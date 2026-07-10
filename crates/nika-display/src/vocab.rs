// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE glyph + hint vocabulary every comprehension surface speaks.
//!
//! One arrow, one hint form (`label: command`), one ASCII-parity seam —
//! the storyboard tail, the trace readers, the failure card and the
//! verb epilogues all pull from here, so the registers can never drift
//! apart glyph-by-glyph.

use crate::theme::{Role, Theme};

/// The data arrow (`→` · `->` under `--ascii`) — output tails, flow
/// edges, outputs pointers.
#[must_use]
pub fn arrow(ascii: bool) -> &'static str {
    if ascii { "->" } else { "→" }
}

/// The comparison mark for "at least" (`≥` · `>=` under `--ascii`) —
/// the audited card's cost floor.
#[must_use]
pub fn at_least(ascii: bool) -> &'static str {
    if ascii { ">=" } else { "≥" }
}

/// One actionable hint: `label: command` — painted dim as a unit. The
/// caller owns indentation; the FORM is the vocabulary (`fix: nika
/// explain NIKA-431` · `re-baseline: nika test wf.yaml --update` ·
/// `explore: nika trace outputs run.ndjson`).
#[must_use]
pub fn hint(theme: Theme, label: &str, command: &str) -> String {
    theme.paint(Role::Dim, &format!("{label}: {command}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, false, false);

    /// Every vocabulary glyph carries its ASCII twin — the parity law.
    #[test]
    fn glyphs_have_ascii_parity() {
        assert_eq!(arrow(false), "→");
        assert_eq!(arrow(true), "->");
        assert_eq!(at_least(false), "≥");
        assert_eq!(at_least(true), ">=");
    }

    /// The hint form is fixed: `label: command`, dim as one unit.
    #[test]
    fn hint_form_is_label_colon_command() {
        assert_eq!(
            hint(PLAIN, "explore", "nika trace outputs run.ndjson"),
            "explore: nika trace outputs run.ndjson"
        );
        let coloured = Theme {
            color: true,
            ..PLAIN
        };
        let painted = hint(coloured, "fix", "nika explain NIKA-431");
        assert!(
            painted.starts_with("\x1b[2m") && painted.ends_with("\x1b[0m"),
            "one dim unit: {painted:?}"
        );
    }
}
