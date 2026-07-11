// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Deterministic « did you mean » — the checker's suggestion surface.
//!
//! The METRIC (Damerau-Levenshtein OSA + the rustc `max(len/3, 1)`
//! threshold) lives in [`nika_types::suggest`] — hoisted 2026-07-11 (the
//! `ip_is_blocked` precedent) so the closed-namespace surfaces OUTSIDE
//! this crate (the provider resolver's MODELS rung · the extract-mode
//! typo rung) suggest with the SAME semantics as the parser/checker.
//! This module re-exports it verbatim for the in-crate callers and keeps
//! the render-side clause helper (schema-only vocabulary).

pub(crate) use nika_types::suggest::{damerau_levenshtein, did_you_mean};

/// Render a suggestion clause (`" — did you mean ___?"`) or empty.
/// A suggestion carrying spaces is a TEACHING sentence (e.g. the modeline
/// cause-namer), not a candidate key — it renders as prose, no question.
pub(crate) fn suggestion_clause(suggestion: Option<&str>) -> String {
    suggestion.map_or_else(String::new, |s| {
        if s.contains(' ') {
            format!(" — {s}")
        } else {
            format!(" — did you mean `{s}`?")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexport_keeps_the_checker_semantics() {
        // The consumer pin: the hoist must not change what THIS crate's
        // callers observe (threshold · tie-break · exact-match silence ·
        // the OSA variant).
        let keys = ["summary", "highlights", "impact"];
        assert_eq!(did_you_mean("sumary", keys), Some("summary"));
        assert_eq!(did_you_mean("zzzzzz", keys), None);
        assert_eq!(did_you_mean("summary", ["summary"]), None);
        assert_eq!(did_you_mean("bd", ["cd", "ad"]), Some("ad"));
        assert_eq!(damerau_levenshtein("ca", "abc"), 3);
    }

    #[test]
    fn clause_renders_key_vs_prose() {
        assert_eq!(
            suggestion_clause(Some("summary")),
            " — did you mean `summary`?"
        );
        assert_eq!(
            suggestion_clause(Some("compute it in a jq task")),
            " — compute it in a jq task"
        );
        assert_eq!(suggestion_clause(None), "");
    }
}
