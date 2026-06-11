// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Deterministic « did you mean » — the suggestion core every check
//! report shares (rustc's diagnostic model · no LLM, no heuristics that
//! vary run-to-run).
//!
//! Distance is **Damerau-Levenshtein** (optimal string alignment): one
//! edit = insert · delete · substitute · transpose-adjacent. Transposition
//! matters because the dominant real-world typo class in field/tool names
//! is swapped letters (`sumary` → wait, that's a delete — `nmae` → `name`
//! IS a transposition). The acceptance threshold is rustc's:
//! `max(len/3, 1)` — short names tolerate 1 edit, long names scale, and
//! anything farther is silence (a WRONG suggestion is worse than none).

/// The best near-match for `target` among `candidates`, when one is
/// close enough to assert. Ties break to the lexicographically smallest
/// candidate (full determinism — input order never changes the answer).
pub(crate) fn did_you_mean<'a, I>(target: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let threshold = (target.chars().count() / 3).max(1);
    let mut best: Option<(usize, &str)> = None;
    for candidate in candidates {
        if candidate == target {
            continue; // an exact match is not a suggestion
        }
        let d = damerau_levenshtein(target, candidate);
        if d > threshold {
            continue;
        }
        best = match best {
            Some((bd, bc)) if (d, candidate) >= (bd, bc) => Some((bd, bc)),
            _ => Some((d, candidate)),
        };
    }
    best.map(|(_, c)| c)
}

/// Render a suggestion clause (`" — did you mean ___?"`) or empty.
pub(crate) fn suggestion_clause(suggestion: Option<&str>) -> String {
    suggestion.map_or_else(String::new, |s| format!(" — did you mean `{s}`?"))
}

/// Damerau-Levenshtein distance (optimal string alignment variant) —
/// the classic O(n·m) dynamic program over chars, with the adjacent-
/// transposition case. Unicode-correct (per-char, not per-byte).
pub(crate) fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    // three rolling rows: i-2, i-1, i
    let mut prev2: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];

    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut d = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
            // adjacent transposition (ab ↔ ba)
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d = d.min(prev2[j - 2] + 1);
            }
            curr[j] = d;
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // Metric-space axioms — a distance that violates these would make
        // did_you_mean rank suggestions nonsensically. Inputs bounded so
        // the O(n·m) DP stays fast.
        #[test]
        fn distance_is_a_pseudometric(
            a in "\\PC{0,40}",
            b in "\\PC{0,40}",
            c in "\\PC{0,40}",
        ) {
            // identity
            prop_assert_eq!(damerau_levenshtein(&a, &a), 0);
            // symmetry
            prop_assert_eq!(damerau_levenshtein(&a, &b), damerau_levenshtein(&b, &a));
            // triangle inequality
            prop_assert!(
                damerau_levenshtein(&a, &c)
                    <= damerau_levenshtein(&a, &b) + damerau_levenshtein(&b, &c)
            );
            // bounded by the longer length (per-char edits)
            let max_len = a.chars().count().max(b.chars().count());
            prop_assert!(damerau_levenshtein(&a, &b) <= max_len);
        }

        // did_you_mean NEVER invents a candidate and NEVER returns the
        // exact target — the two invariants the agent loop relies on.
        #[test]
        fn did_you_mean_only_ever_returns_a_real_non_exact_candidate(
            target in "[a-z_]{1,16}",
            candidates in prop::collection::vec("[a-z_]{1,16}", 0..8),
        ) {
            let refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
            if let Some(hit) = did_you_mean(&target, refs.iter().copied()) {
                prop_assert!(candidates.iter().any(|c| c == hit), "invented `{hit}`");
                prop_assert_ne!(hit, target.as_str(), "exact match is not a suggestion");
            }
        }
    }

    #[test]
    fn distance_basics() {
        assert_eq!(damerau_levenshtein("summary", "summary"), 0);
        assert_eq!(damerau_levenshtein("sumary", "summary"), 1); // delete
        assert_eq!(damerau_levenshtein("nmae", "name"), 1); // transpose
        assert_eq!(damerau_levenshtein("titel", "title"), 1); // transpose
        assert_eq!(damerau_levenshtein("", "abc"), 3);
        assert_eq!(damerau_levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn unicode_is_per_char_not_per_byte() {
        // é is 2 bytes — a byte-based DP would count 2 edits.
        assert_eq!(damerau_levenshtein("café", "cafe"), 1);
    }

    #[test]
    fn osa_variant_is_pinned() {
        // THE distinguisher: full Damerau-Levenshtein gives 2 for
        // "ca"→"abc" (transpose then insert INTO the transposed pair);
        // optimal string alignment gives 3 (no edit-within-transposition).
        // The doc contracts OSA — pin it so a refactor can't silently
        // change which variant ships.
        assert_eq!(damerau_levenshtein("ca", "abc"), 3);
        assert_eq!(damerau_levenshtein("abc", "ca"), 3);
    }

    #[test]
    fn did_you_mean_respects_the_threshold() {
        let keys = ["summary", "highlights", "impact"];
        assert_eq!(did_you_mean("sumary", keys), Some("summary"));
        assert_eq!(did_you_mean("highlihgts", keys), Some("highlights"));
        // far from everything → silence, never a wrong guess
        assert_eq!(did_you_mean("zzzzzz", keys), None);
        // short target: threshold 1 — `impct` is 1 away, suggested
        assert_eq!(did_you_mean("impct", keys), Some("impact"));
    }

    #[test]
    fn ties_break_lexicographically_deterministic() {
        // both 1 edit away from `bd` — `ad` < `cd` wins, whatever the
        // candidate iteration order.
        assert_eq!(did_you_mean("bd", ["cd", "ad"]), Some("ad"));
        assert_eq!(did_you_mean("bd", ["ad", "cd"]), Some("ad"));
    }

    #[test]
    fn exact_match_is_not_a_suggestion() {
        assert_eq!(did_you_mean("summary", ["summary"]), None);
    }
}
