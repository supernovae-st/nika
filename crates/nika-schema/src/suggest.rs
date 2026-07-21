// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Deterministic « did you mean » — the checker's suggestion surface.
//!
//! The metric (Damerau-Levenshtein OSA + the rustc `max(len/3, 1)`
//! threshold) AND the render clause live in [`nika_types::suggest`] —
//! the metric hoisted 2026-07-11 (the `ip_is_blocked` precedent), the
//! clause descended at the C2 wall (the 15k prod-LOC budget). This shim
//! re-exports the door verbatim for the in-crate callers.

pub(crate) use nika_types::suggest::{damerau_levenshtein, did_you_mean, suggestion_clause};

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
