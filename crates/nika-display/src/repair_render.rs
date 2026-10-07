// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Passive repair reports: producer-owned facts in, themed text out.
//! Parsing, applying, rollback, round limits, bookkeeping and effects stay with the host.
//! These publicly constructible records describe decisions; they do not authorize them.

use crate::theme::{Role, Theme};
use std::fmt::Write as _;

/// One applied (or skipped) repair, for the summary.
#[derive(Clone, Debug)]
pub struct Repair {
    /// The dead form the repair replaces (the summary's left side).
    pub old: String,
    /// The repaired form (the summary's right side).
    pub new: String,
    /// The ladder kind (`w1-map` · `w2-flow` · `d1-split` · …).
    pub kind: &'static str,
    /// Whether the repair landed (a skip stays retryable — a later
    /// round's splice can make the token unique).
    pub applied: bool,
}

impl Repair {
    /// One APPLIED repair row (the 15k-wall constructor — the splice
    /// site sets its own flag from the gate, hence not this).
    #[must_use]
    pub fn applied(old: &str, new: &str, kind: &'static str) -> Self {
        Self {
            old: old.to_owned(),
            new: new.to_owned(),
            kind,
            applied: true,
        }
    }
}

/// Equivalence-or-stop diagnostics (W2 · D1) — rendered verbatim.
#[derive(Clone, Debug)]
pub struct StopNotes(pub Vec<String>);

/// A round the loop REFUSED to commit: the transformed text no longer
/// loaded as YAML (syntax or duplicate-key refusal) although the text it
/// started from did. The round is
/// rolled back to its savepoint (the file is never written from it) and
/// this row says what was attempted and why it was refused — a typed
/// refusal, never a silent write of a document `check` cannot read.
///
/// The invariant it enforces (2026-08-18): if `--fix` reports a repair,
/// the document on disk parses at least as far as the document it
/// replaced. Measured before the gate: the shipped 0.108.0 spliced a
/// teaching sentence into a key, announced « 1 repair applied » and
/// left YAML that no longer parsed.
#[derive(Clone, Debug)]
pub struct Refusal {
    /// The repairs the round would have applied (`kind old → new` rows).
    pub attempted: Vec<String>,
    /// The parse failure the transformed text produced.
    pub reason: String,
}

/// Render the refusal rows (one per rolled-back round · refuse glyph).
#[must_use]
pub fn render_refusals(refusals: &[Refusal], theme: Theme) -> String {
    let mut out = String::new();
    for r in refusals {
        let _ = writeln!(
            out,
            " {} {}  refused — {} · the repaired text does not parse ({}) · the file is unchanged",
            theme.paint(Role::Bad, "✗"),
            theme.paint(Role::Strong, "FIX"),
            r.attempted.join(" · "),
            r.reason,
        );
    }
    out
}

/// Render the STOP diagnostic lines (verbatim W2/D1 notes · warn glyph).
#[must_use]
pub fn render_stops(stop_notes: &StopNotes, theme: Theme) -> String {
    let mut stops = String::new();
    for note in &stop_notes.0 {
        let _ = writeln!(
            stops,
            " {} {}  {note}",
            theme.paint(Role::Warn, "◼"),
            theme.paint(Role::Strong, "STOP"),
        );
    }
    stops
}

/// Per-repair lines + the closing verdict (count or the honest note).
#[must_use]
pub fn summary(repairs: &[Repair], applied: usize, theme: Theme) -> String {
    let mut out = String::new();
    for r in repairs {
        if r.applied {
            let _ = writeln!(
                out,
                " {} {}  {} `{}` → `{}`",
                theme.paint(Role::Good, "✔"),
                theme.paint(Role::Strong, "FIX"),
                r.kind,
                r.old,
                r.new,
            );
        } else {
            let _ = writeln!(
                out,
                " {} {}  {} `{}` → `{}` skipped — `{}` is not unique in the file \
                 (a blind splice could rewrite the wrong site)",
                theme.paint(Role::Dim, "○"),
                theme.paint(Role::Strong, "FIX"),
                r.kind,
                r.old,
                r.new,
                r.old,
            );
        }
    }
    if applied == 0 {
        let _ = writeln!(
            out,
            " {} {}  no machine-applicable repairs (typed rename suggestions only \
             — structural findings stay yours)",
            theme.paint(Role::Dim, "○"),
            theme.paint(Role::Strong, "FIX"),
        );
    } else {
        let plural = if applied == 1 { "repair" } else { "repairs" };
        let _ = writeln!(
            out,
            " {} {}  {applied} {plural} applied · re-audit below",
            theme.paint(Role::Good, "✔"),
            theme.paint(Role::Strong, "FIX"),
        );
    }
    out
}

#[cfg(test)]
mod tests;
