// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Query-side surface · `score(query, doc_id)` + `top_k(query, k)`.
//!
//! Discipline · per ADR-078 `&self` after finalize · pure read.

use crate::BmIndex;
use crate::scorer::term_score;
use crate::tokenize::tokenize;

impl BmIndex {
    /// Score a single document against a query.
    ///
    /// Returns `None` if the index is empty OR `Some(0.0)` when the query
    /// has zero token overlap with the document vocabulary.
    ///
    /// MUST be called after [`Self::finalize`]; otherwise the returned
    /// score is computed against stale IDF (callers SHOULD check
    /// [`Self::is_finalized`]).
    #[must_use]
    pub fn score(&self, query: &str, doc_id: u32) -> Option<f64> {
        if self.doc_count() == 0 {
            return None;
        }
        let (tf, doc_len) = self.doc(doc_id)?;
        let params = self.params();
        let avgdl = self.avgdl();
        let total: f64 = tokenize(query)
            .into_iter()
            .map(|term| {
                let idf = self.term_idf(&term);
                let term_tf = tf.get(&term).copied().unwrap_or(0);
                term_score(
                    term_tf,
                    doc_len,
                    avgdl,
                    idf,
                    params.k1,
                    params.b,
                    params.delta,
                )
            })
            .sum();
        Some(total)
    }

    /// Top-k retrieval · returns `(doc_id, score)` sorted descending by score.
    ///
    /// Stable tiebreak by ascending `doc_id`. Empty index returns `Vec::new()`.
    ///
    /// MUST be called after [`Self::finalize`].
    #[must_use]
    pub fn top_k(&self, query: &str, k: usize) -> Vec<(u32, f64)> {
        if self.doc_count() == 0 || k == 0 {
            return Vec::new();
        }
        let qtokens = tokenize(query);
        if qtokens.is_empty() {
            return Vec::new();
        }
        let params = self.params();
        let avgdl = self.avgdl();
        let mut scored: Vec<(u32, f64)> = self
            .docs_iter()
            .map(|(id, tf, doc_len)| {
                let total: f64 = qtokens
                    .iter()
                    .map(|term| {
                        let idf = self.term_idf(term);
                        let term_tf = tf.get(term).copied().unwrap_or(0);
                        term_score(
                            term_tf,
                            doc_len,
                            avgdl,
                            idf,
                            params.k1,
                            params.b,
                            params.delta,
                        )
                    })
                    .sum();
                (id, total)
            })
            .collect();
        // Sort descending by score · stable tiebreak by doc_id ascending.
        // Per rust-pro 2026 SOTA audit · `f64::total_cmp` (stable 1.62+) is the
        // canonical form for finite-by-construction scores · NaN would sort to
        // a definite position (closing the silent-coalesce gap that
        // `partial_cmp().unwrap_or(Equal)` had · Gate 5 mutation hardening).
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.truncate(k);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BmParams;

    fn manning() -> BmIndex {
        let mut idx = BmIndex::new(BmParams::default());
        idx.add_document(1, "auto car insurance");
        idx.add_document(2, "best auto insurance");
        idx.add_document(3, "car insurance best auto");
        idx.add_document(4, "insurance best car");
        idx.finalize();
        idx
    }

    #[test]
    fn empty_index_score_none() {
        let idx = BmIndex::new(BmParams::default());
        assert!(idx.score("anything", 1).is_none());
    }

    #[test]
    fn empty_index_top_k_empty() {
        let idx = BmIndex::new(BmParams::default());
        assert!(idx.top_k("anything", 5).is_empty());
    }

    #[test]
    fn zero_k_returns_empty() {
        let idx = manning();
        assert!(idx.top_k("auto", 0).is_empty());
    }

    #[test]
    fn zero_overlap_zero_score() {
        let idx = manning();
        assert_eq!(idx.score("xyzzy plover", 1), Some(0.0));
    }

    #[test]
    fn unknown_doc_id_none() {
        let idx = manning();
        assert!(idx.score("auto", 999).is_none());
    }

    #[test]
    fn top_k_returns_at_most_k() {
        let idx = manning();
        assert_eq!(idx.top_k("auto", 2).len(), 2);
        assert_eq!(idx.top_k("auto", 100).len(), 4);
    }

    #[test]
    fn top_k_descending_by_score() {
        let idx = manning();
        let r = idx.top_k("best car insurance", 4);
        // All scores finite + non-negative
        for (_, s) in &r {
            assert!(s.is_finite());
            assert!(*s >= 0.0);
        }
        // Descending
        for w in r.windows(2) {
            assert!(w[0].1 >= w[1].1);
        }
    }
}
