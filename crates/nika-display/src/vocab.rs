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
/// speaks (`≤N tk` per task, "worst-case output ceiling" on the range).
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

/// The `--ascii` enforcement seam (audit UX 2026-07-30 · P1 « --plain
/// --ascii réellement ASCII »). The glyph twins (`Theme::glyph` ·
/// `verb_glyph` · [`arrow`] · [`at_most`]) are the PRIMARY mechanism —
/// but a hardcoded literal (`·` · `—` · `→` · `↳` · `⭐` · `✖`) rides
/// no seam, and every one the audit caught was exactly that class. A
/// surface that opted into the ASCII register hands its FINISHED text
/// through here once, at the verb boundary: the chrome punctuation and
/// every documented twin fold to their ASCII spelling, so the emitted
/// bytes are ASCII by construction rather than by audit.
///
/// The fold set is CHROME ONLY — decorative punctuation and the glyphs
/// that already have a `Theme` twin. Letters and digits are never
/// touched: content (a task id · a path · a model name) renders as
/// written. A no-op unless `theme.ascii` — the unicode register keeps
/// its exact bytes.
#[must_use]
pub fn sober(theme: Theme, text: &str) -> String {
    if !theme.ascii {
        return text.to_owned();
    }
    text.chars()
        .flat_map(|c| match c {
            // The separator/dash family — one twin, one arm (clippy's
            // same-arms law): `·` the separator · en `–` the cost range ·
            // the box-drawing horizontals (chrome.rs panel/rail literals).
            '·' | '–' | '─' | '━' | '╸' => "-".chars().collect(),
            // The dash family (em — the prose dash).
            '—' => "--".chars().collect(),
            // The arrow family (data arrow · hint marker).
            '→' | '↳' => "->".chars().collect(),
            // The comparison family (mirrors at_least/at_most).
            '≥' => ">=".chars().collect(),
            '≤' => "<=".chars().collect(),
            // Ellipsis.
            '…' => "...".chars().collect(),
            // The verdict + brand glyphs (mirrors mark()/logo()).
            '✔' => "ok".chars().collect(),
            '✖' => "X".chars().collect(),
            '⚠' => "!".chars().collect(),
            '🦋' => "[nika]".chars().collect(),
            '○' => ".".chars().collect(),
            // The pointer pair (the running-state glyph · the welcome
            // cascade's selected rung — one twin, so one arm: clippy's
            // same-arms law). `▸` rode no `Theme` seam AND was in no
            // arm, so it leaked through both layers at once and `main`
            // went red on it (#1193).
            '◐' | '▸' => ">".chars().collect(),
            '↻' => "r".chars().collect(),
            '↷' => "~>".chars().collect(),
            '⊘' => "x".chars().collect(),
            '◇' => "i".chars().collect(),
            '▷' => "$".chars().collect(),
            '◆' => "@".chars().collect(),
            // The star pair (the GitHub mark · the agent verb glyph —
            // one twin, so one arm: clippy's same-arms law).
            '⭐' | '✦' => "*".chars().collect(),
            '│' => "|".chars().collect(),
            '╭' | '╮' | '╰' | '╯' | '┌' | '┐' | '└' | '┘' => "+".chars().collect(),
            other => vec![other],
        })
        .collect()
}

/// A USD amount at the 4-decimal display grain, ceiling-honest at the
/// bottom: a positive amount smaller than the grain rounds UP to
/// `0.0001` instead of printing `0.0000` — a fabricated zero for money
/// the run WILL spend (probe 2026-07-30: a couple of output tokens on a
/// priced model bill real money under an `est out ≤$0.0000` card; the
/// ENERGY lane's display grain paid for the same lesson first). An
/// EXACT zero — a provably-never-runs task, a declared zero cap — still
/// prints `0.0000`: that zero is true.
#[must_use]
pub fn usd(amount: f64) -> String {
    if amount > 0.0 && amount < 0.000_05 {
        "0.0001".to_owned()
    } else {
        format!("{amount:.4}")
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

    /// The `--ascii` enforcement seam: a surface's finished text folds
    /// every chrome glyph the audit caught (· — → ↳ ⭐ ✔ ✖ ⚠ 🦋 · the
    /// box-drawing panel set) to its documented ASCII twin — the bytes
    /// are ASCII by construction, and the twins match what the `Theme`
    /// seam would have picked (one vocabulary, two mechanisms).
    #[test]
    fn sober_folds_the_chrome_set_to_ascii() {
        let ascii = Theme::new(false, true, false);
        let leaked = " ✖ CONFORM · findings — read → fix ↳ [NIKA-1] · est ≤$0.01 · ⭐ 🦋 ⚠ …";
        let folded = sober(ascii, leaked);
        assert!(
            folded.is_ascii(),
            "every byte < 128 after the fold: {folded:?}"
        );
        assert_eq!(
            folded,
            " X CONFORM - findings -- read -> fix -> [NIKA-1] - est <=$0.01 - * [nika] ! ..."
        );
        // The state/verb/box glyphs fold to the SAME twins Theme picks.
        let glyphs = sober(ascii, "○ ◐ ↻ ↷ ⊘ ◇ ▷ ◆ ✦ ─ │ ╭ ╯");
        assert_eq!(glyphs, ". > r ~> x i $ @ * - | + +");
    }

    /// The unicode register keeps its exact bytes — `sober` is a no-op
    /// unless the theme opted into ASCII (a fold that always fires would
    /// sober the default surface, the opposite regression).
    #[test]
    fn sober_is_a_no_op_outside_the_ascii_register() {
        let text = "✔ audited · 2 tasks — est ≤$0.01 · → nika run";
        assert_eq!(sober(PLAIN, text), text, "unicode keeps its bytes");
        // Content is never transliterated: letters survive the fold.
        let ascii = Theme::new(false, true, false);
        assert_eq!(
            sober(ascii, "task `café` reads ./données"),
            "task `café` reads ./données",
            "only the chrome set folds — content renders as written"
        );
    }
}
