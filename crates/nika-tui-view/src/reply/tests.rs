// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A reply's Markdown as a person reads it, one line for each of its lines.

use ratatui::style::Modifier;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

use super::{lines, lines_within};

/// Each line's words, joined.
fn words(lines: &[Vec<Span<'_>>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.iter().map(|span| span.content.as_ref()).collect())
        .collect()
}

/// The span holding `text` exactly, in `lines`.
fn span<'a>(lines: &'a [Vec<Span<'_>>], text: &str) -> Option<&'a Span<'a>> {
    lines.iter().flatten().find(|span| span.content == text)
}

/// Strong and emphasis lose their markers to a weight and a slant, a code span keeps its
/// backticks, a heading reads as its title in weight, and a list item stands behind a bullet:
/// the reply keeps one line for each of its own, and every word.
#[test]
fn a_reply_reads_its_marks_as_weights_and_its_lists_behind_bullets() {
    let reply = "## Résumé\nJ'ai **lu** les *trois* flux avec `nika:fetch`.\n- Le Monde\n  - Ars\n2. The Verge";
    let shown = lines(reply, false, false);
    assert_eq!(
        words(&shown),
        [
            "Résumé",
            "J'ai lu les trois flux avec `nika:fetch`.",
            "• Le Monde",
            "  • Ars",
            "2. The Verge",
        ]
    );
    let bold = |text: &str| {
        span(&shown, text).is_some_and(|s| s.style.add_modifier.contains(Modifier::BOLD))
    };
    assert!(bold("Résumé") && bold("lu"), "{shown:?}");
    let slanted =
        span(&shown, "trois").is_some_and(|s| s.style.add_modifier.contains(Modifier::ITALIC));
    assert!(slanted, "{shown:?}");
    let ascii = lines("- Le Monde\n- Ars", false, true);
    assert_eq!(words(&ascii), ["- Le Monde", "- Ars"], "ASCII bullets");
}

/// A simple table reads as aligned columns, its separator a rule and its header in weight, one
/// line for each of its rows; the ASCII column draws ASCII bars and rules. A pipe that opens no
/// table stays as written, and fenced code is never read as Markdown: it reads between quiet
/// rules where its fences stood, never their backticks (live gauntlet 5, r08 B12), the first
/// naming its language.
#[test]
fn a_simple_table_reads_as_aligned_columns_and_code_stays_as_written() {
    let reply = "| Source | Articles |\n|---|---:|\n| Le Monde | 12 |\n| **Ars** | 3 |\nfin";
    let shown = lines(reply, false, false);
    assert_eq!(
        words(&shown),
        [
            "Source   │ Articles",
            "─────────┼─────────",
            "Le Monde │ 12",
            "Ars      │ 3",
            "fin",
        ]
    );
    let header =
        span(&shown, "Source").is_some_and(|s| s.style.add_modifier.contains(Modifier::BOLD));
    assert!(header, "{shown:?}");
    let ascii = lines(reply, false, true);
    assert_eq!(
        words(&ascii)[..2],
        ["Source   | Articles", "---------+---------"]
    );
    let lone = lines("| not a table\nnext", false, false);
    assert_eq!(words(&lone), ["| not a table", "next"]);
    let code = "```\n**kept** | as | written\n- no bullet\n```";
    assert_eq!(
        words(&lines(code, false, false)),
        ["───", "**kept** | as | written", "- no bullet", "───"]
    );
    let named = "Voici le fichier :\n~~~text\nlundi: 3 tâches\n~~~\nfin";
    assert_eq!(
        words(&lines(named, false, true)),
        [
            "Voici le fichier :",
            "--- text",
            "lundi: 3 tâches",
            "---",
            "fin"
        ]
    );
    let rule = lines(named, true, false);
    let dim = crate::visual::role::style(nika_display::theme::Role::Dim, true);
    assert!(
        span(&rule, "─── text").is_some_and(|s| s.style == dim),
        "{rule:?}"
    );
}

/// A table wider than its row (live gauntlet 4, r09 B14) keeps one line for each of its rows
/// rather than wrapping them: its widest column narrows, a cell wider than its column is cut on
/// the cut mark (the header in weight still), the narrow columns stay whole, and no row is wider
/// than the row it is given, down to a row too narrow for every column. The ASCII column cuts
/// on its own mark; a table that fits keeps its natural columns.
#[test]
fn a_table_wider_than_its_row_narrows_its_widest_column_rather_than_wrapping() {
    let reply = "| Mois | Total (EUR) | Croissance vs mois précédent |\n|---|---:|---:|\n\
                 | 2026-01 | 86273 | — |\n| 2026-02 | 87419 | 1.33 % |";
    let shown = lines_within(reply, false, false, 40);
    assert_eq!(
        words(&shown),
        [
            "Mois    │ Total (EUR) │ Croissance vs m…",
            "────────┼─────────────┼─────────────────",
            "2026-01 │ 86273       │ —",
            "2026-02 │ 87419       │ 1.33 %",
        ]
    );
    let header = span(&shown, "Croissance vs m")
        .is_some_and(|s| s.style.add_modifier.contains(Modifier::BOLD));
    assert!(header, "{shown:?}");
    let ascii = lines_within(reply, false, true, 40);
    assert_eq!(words(&ascii)[0], "Mois    | Total (EUR) | Croissance vs...");
    for width in [12, 5] {
        let narrow = words(&lines_within(reply, false, false, width));
        assert_eq!(narrow.len(), 4, "{narrow:?}");
        assert!(
            narrow.iter().all(|row| row.width() <= width),
            "{width}: {narrow:?}"
        );
    }
    assert_eq!(
        lines_within(reply, false, false, 52),
        lines(reply, false, false)
    );
}
