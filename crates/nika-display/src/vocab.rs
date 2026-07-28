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
/// for a genuine LOWER bound.
///
/// It used to carry the audited card's cost line, which was never a
/// floor: that number is the cheapest PATH with every task priced at its
/// own token cap, so a run bills under it routinely (measured: $0.000242
/// against an announced `≥$0.0305`). That line now speaks [`at_most`].
#[must_use]
pub fn at_least(ascii: bool) -> &'static str {
    if ascii { ">=" } else { "≥" }
}

/// The comparison mark for "at most" (`≤` · `<=` under `--ascii`) — for
/// a genuine UPPER bound, the shape the whole COST section already
/// speaks (`≤N tk` per task, "worst-case ceiling" on the range).
#[must_use]
pub fn at_most(ascii: bool) -> &'static str {
    if ascii { "<=" } else { "≤" }
}

/// One actionable hint: `label: command` — painted dim as a unit. The
/// caller owns indentation; the FORM is the vocabulary (`fix: nika
/// explain NIKA-431` · `re-baseline: nika test wf.yaml --update` ·
/// `explore: nika trace outputs run.ndjson`).
#[must_use]
pub fn hint(theme: Theme, label: &str, command: &str) -> String {
    theme.paint(Role::Dim, &format!("{label}: {command}"))
}

/// Count-noun agreement: `count(3, "task")` → `3 tasks` · `count(1,
/// "task")` → `1 task` — the surface reads as prose, never `1 tasks`
/// nor the lazy `task(s)`. Two rules cover the band's whole noun set
/// (task · wave · run · retry · edge · finding): consonant+`y` → `ies`
/// (`1 retry` · `2 retries`), else a plain `s`. A compound noun
/// pluralizes at its tail (`4 downstream tasks`).
#[must_use]
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        return format!("1 {noun}");
    }
    let ies = noun.ends_with('y')
        && !noun
            .chars()
            .rev()
            .nth(1)
            .is_some_and(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u'));
    if ies {
        format!("{n} {}ies", &noun[..noun.len() - 1])
    } else {
        format!("{n} {noun}s")
    }
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
        assert_eq!(at_most(false), "≤");
        assert_eq!(at_most(true), "<=");
    }

    /// One reads singular, the rest plural — noun tail included, and
    /// consonant+y takes `ies` (`retries`) while vowel+y stays (`days`).
    #[test]
    fn count_agrees_in_number() {
        assert_eq!(count(1, "task"), "1 task");
        assert_eq!(count(0, "task"), "0 tasks");
        assert_eq!(count(3, "wave"), "3 waves");
        assert_eq!(count(1, "downstream task"), "1 downstream task");
        assert_eq!(count(1, "retry"), "1 retry");
        assert_eq!(count(0, "retry"), "0 retries");
        assert_eq!(count(2, "day"), "2 days");
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
