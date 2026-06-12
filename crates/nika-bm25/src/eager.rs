// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Eager sparse scoring — the BM25S architecture (Lù 2024 *BM25S:
//! Orders of magnitude faster lexical search via eager sparse scoring*
//! · arxiv.org/abs/2407.03618).
//!
//! The insight: once the corpus is frozen, a term's BM25 contribution
//! to a document is FULLY determined — `idf(t) · saturation(tf, |d|,
//! avgdl, k1, b)` has no query-time inputs. So compute every
//! `(term, doc) → score` ONCE at build time and make a query a pure
//! sparse accumulation over the postings of its terms:
//!
//! - documents containing none of the query terms are never touched
//!   (the lazy path visits all N docs per query — `O(N·|Q|)` dense),
//! - no floating-point math per posting at query time beyond `+=`.
//!
//! Exact-equivalence discipline: the accumulator adds postings in
//! QUERY-TOKEN order, which is the same addition order as the lazy
//! [`BmIndex::top_k`] per-document sum — IEEE addition is
//! non-associative, so matching the ORDER is what makes the two paths
//! byte-identical (pinned by the equivalence property below), not just
//! approximately equal. Absent terms contribute exactly `+0.0` in the
//! lazy path, which is an identity on non-negative scores.

use std::collections::BTreeMap;

use crate::BmIndex;
use crate::tokenize::tokenize;

/// A frozen, eagerly-scored BM25 index (BM25S form).
///
/// Built FROM a finalized [`BmIndex`] — the separate type IS the
/// freshness contract: it cannot exist unfinalized, and a later
/// `add_document` on the source index cannot silently stale it (it
/// holds its own postings).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EagerIndex {
    /// term → postings `(doc_id, full precomputed BM25 contribution)`,
    /// doc-sorted (`BTreeMap` source order · deterministic).
    postings: BTreeMap<String, Vec<(u32, f64)>>,
    /// Number of documents in the frozen corpus.
    doc_count: usize,
}

impl EagerIndex {
    /// Build the eager table from a FINALIZED index. Returns `None`
    /// when the index has not been finalized (scoring against stale
    /// IDF must stay unrepresentable, not just discouraged).
    #[must_use]
    pub fn build(index: &BmIndex) -> Option<Self> {
        if !index.is_finalized() {
            return None;
        }
        let params = index.params();
        let avgdl = index.avgdl();
        let mut postings: BTreeMap<String, Vec<(u32, f64)>> = BTreeMap::new();
        for (doc_id, tf, doc_len) in index.docs_iter() {
            for (term, &freq) in tf {
                let score = crate::scorer::term_score(
                    freq,
                    doc_len,
                    avgdl,
                    index.term_idf(term),
                    params.k1,
                    params.b,
                    params.delta,
                );
                postings
                    .entry(term.clone())
                    .or_default()
                    .push((doc_id, score));
            }
        }
        Some(Self {
            postings,
            doc_count: index.doc_count(),
        })
    }

    /// Number of documents in the frozen corpus.
    #[must_use]
    pub fn doc_count(&self) -> usize {
        self.doc_count
    }

    /// Top-k retrieval over the eager table — sparse: only documents
    /// containing at least one query term are visited. Same contract
    /// as [`BmIndex::top_k`] (descending score · ascending `doc_id`
    /// tiebreak) and byte-identical scores (see the module doc).
    #[must_use]
    pub fn top_k(&self, query: &str, k: usize) -> Vec<(u32, f64)> {
        // no emptiness guards ON PURPOSE: an empty corpus has empty
        // postings, an empty query skips the loop, and `k == 0`
        // truncates to nothing — every degenerate case falls through
        // the same code path (fewer branches · nothing to mutate)
        let qtokens = tokenize(query);
        // accumulate in QUERY-TOKEN order (the lazy path's per-doc
        // addition order — exact float equivalence depends on it)
        let mut acc: BTreeMap<u32, f64> = BTreeMap::new();
        for term in &qtokens {
            if let Some(rows) = self.postings.get(term) {
                for &(doc, score) in rows {
                    *acc.entry(doc).or_insert(0.0) += score;
                }
            }
        }
        let mut scored: Vec<(u32, f64)> = acc.into_iter().collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.truncate(k);
        scored
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)] // test-scope: a bad fixture IS the failure

    use super::*;
    use crate::BmParams;

    fn corpus() -> BmIndex {
        let mut idx = BmIndex::new(BmParams::default());
        idx.add_document(1, "auto car insurance");
        idx.add_document(2, "best auto insurance");
        idx.add_document(3, "car insurance best auto");
        idx.add_document(4, "insurance best car");
        idx.add_document(5, "unrelated text entirely");
        idx.finalize();
        idx
    }

    #[test]
    fn build_requires_finalize() {
        let mut idx = BmIndex::new(BmParams::default());
        idx.add_document(1, "auto");
        assert!(EagerIndex::build(&idx).is_none(), "unfinalized must refuse");
        idx.finalize();
        assert!(EagerIndex::build(&idx).is_some());
    }

    #[test]
    fn eager_equals_lazy_exactly_on_a_fixed_corpus() {
        let idx = corpus();
        let eager = EagerIndex::build(&idx).expect("finalized");
        for query in [
            "best car insurance",
            "auto",
            "unrelated text",
            "car car insurance", // duplicate query token: multiplicity counts
            "missing tokens only zzz",
        ] {
            let lazy: Vec<(u32, f64)> = idx
                .top_k(query, 10)
                .into_iter()
                .filter(|(_, s)| *s > 0.0)
                .collect();
            let fast = eager.top_k(query, 10);
            assert_eq!(lazy, fast, "divergence on {query:?}");
        }
    }

    #[test]
    fn sparse_path_never_surfaces_zero_score_docs() {
        // doc 5 shares no vocabulary with the query — the sparse path
        // must not even visit it (the lazy path scores it 0.0)
        let idx = corpus();
        let eager = EagerIndex::build(&idx).expect("finalized");
        let hits = eager.top_k("best car insurance", 10);
        assert!(hits.iter().all(|(id, _)| *id != 5), "{hits:?}");
        assert!(hits.iter().all(|(_, s)| *s > 0.0));
    }

    #[test]
    fn rebuild_after_mutation_sees_the_new_corpus() {
        let mut idx = corpus();
        let before = EagerIndex::build(&idx).expect("finalized");
        idx.add_document(6, "car insurance premium");
        // the source index re-opened: a NEW eager build must refuse
        // until finalize, and the OLD one still answers over its frozen
        // corpus (it cannot silently stale)
        assert!(EagerIndex::build(&idx).is_none());
        assert_eq!(before.doc_count(), 5);
        idx.finalize();
        let after = EagerIndex::build(&idx).expect("re-finalized");
        assert_eq!(after.doc_count(), 6);
        assert!(after.top_k("premium", 3).iter().any(|(id, _)| *id == 6));
    }
}
