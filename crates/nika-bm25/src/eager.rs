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
//! **Dynamic pruning** ([`EagerIndex::top_k_pruned`]): the `MaxScore`
//! family (Turtle & Flood 1995 *Query evaluation: strategies and
//! optimizations*; the arXiv-accessible contemporary anchor is Qiao,
//! Yang, Lin & Yang 2023 *Optimizing Guided Traversal for Fast Learned
//! Sparse Retrieval* · arxiv.org/abs/2305.01203, which builds on
//! `MaxScore`-based skipping). Each term stores its maximum posting
//! score at build time; document-at-a-time traversal splits terms into
//! ESSENTIAL (their max-scores can still beat the current threshold)
//! and NON-ESSENTIAL (alone they cannot) — a document is fully scored
//! only when its essential upper bound clears the threshold. EXACT:
//! pruning uses strict `<` against the current k-th score, so
//! equal-score candidates are always evaluated and the tiebreak
//! (ascending `doc_id`) is preserved — pinned by an equivalence
//! property against the dense path.
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
    /// term → max posting score (the `MaxScore` upper bound · built once).
    max_score: BTreeMap<String, f64>,
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
        let max_score = postings
            .iter()
            .map(|(term, rows)| {
                let m = rows.iter().map(|&(_, s)| s).fold(0.0_f64, f64::max);
                (term.clone(), m)
            })
            .collect();
        Some(Self {
            postings,
            max_score,
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

/// One prepared query term: (term, query-weight, postings).
type TermRow<'e> = (&'e str, f64, &'e [(u32, f64)]);

/// Traversal statistics — lets a test PROVE pruning happened (visited
/// postings strictly below the dense count on a skewed corpus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct PruneStats {
    /// Postings actually scored.
    pub postings_visited: usize,
    /// Postings the dense path would have scored (Σ posting lengths of
    /// the query terms, with multiplicity).
    pub postings_dense: usize,
}

impl EagerIndex {
    /// Top-k with `MaxScore` dynamic pruning — RANK-EXACT vs
    /// [`Self::top_k`] (same documents, same order), with scores equal
    /// up to IEEE addition order: pruning evaluates terms in
    /// cap-sorted order (essential set first) where the dense path
    /// adds in query-token order — non-associativity makes a ≤1-ULP
    /// score drift unavoidable BY DESIGN (the term reorder IS the
    /// algorithm). The conformance suite pins rank-exactness + 1e-9
    /// relative score agreement; fewer postings visited, measured.
    #[must_use]
    pub fn top_k_pruned(&self, query: &str, k: usize) -> Vec<(u32, f64)> {
        self.top_k_pruned_stats(query, k).0
    }

    /// Prepare the query: distinct terms with weights, sorted by
    /// weighted max-score ASCENDING (the `MaxScore` order — the
    /// non-essential prefix comes first), the cap prefix-sums, and the
    /// dense posting count (for the stats).
    fn prepare_query<'e>(&'e self, qtokens: &'e [String]) -> (Vec<TermRow<'e>>, Vec<f64>, usize) {
        // collapse duplicate query tokens into a weight (the dense path
        // adds the posting once per occurrence — weight preserves it)
        let mut weights: BTreeMap<&str, f64> = BTreeMap::new();
        for t in qtokens {
            *weights.entry(t.as_str()).or_insert(0.0) += 1.0;
        }
        let mut terms: Vec<TermRow<'e>> = weights
            .iter()
            .filter_map(|(&term, &w)| {
                self.postings
                    .get(term)
                    .map(|rows| (term, w, rows.as_slice()))
            })
            .collect();
        terms.sort_by(|a, b| {
            let ca = self.max_score.get(a.0).copied().unwrap_or(0.0) * a.1;
            let cb = self.max_score.get(b.0).copied().unwrap_or(0.0) * b.1;
            ca.total_cmp(&cb)
        });
        let dense: usize = qtokens
            .iter()
            .filter_map(|t| self.postings.get(t.as_str()).map(Vec::len))
            .sum();
        // prefix[i] = Σ caps[0..i] — the non-essential upper bound when
        // the essential set starts at i
        let mut prefix = vec![0.0_f64; terms.len() + 1];
        for (i, (t, w, _)) in terms.iter().enumerate() {
            prefix[i + 1] = prefix[i] + self.max_score.get(*t).copied().unwrap_or(0.0) * w;
        }
        (terms, prefix, dense)
    }

    /// [`Self::top_k_pruned`] plus traversal statistics.
    #[must_use]
    pub fn top_k_pruned_stats(&self, query: &str, k: usize) -> (Vec<(u32, f64)>, PruneStats) {
        let qtokens = tokenize(query);
        let (terms, prefix, dense) = self.prepare_query(&qtokens);

        let mut visited = 0usize;
        // current top-k, kept sorted (score desc · doc asc); θ = worst
        let mut top: Vec<(u32, f64)> = Vec::with_capacity(k.saturating_add(1));
        let theta = |top: &Vec<(u32, f64)>| -> Option<f64> {
            (top.len() == k).then(|| top.last().map_or(0.0, |&(_, s)| s))
        };
        // per-term cursor into its postings (doc-sorted)
        let mut cursors = vec![0usize; terms.len()];
        loop {
            let th = theta(&top);
            // essential boundary: smallest e with prefix[e] (the bound
            // of terms BELOW e) unable to beat θ alone — terms ≥ e are
            // essential drivers
            let e = match th {
                None => 0, // top not full yet: every term is essential
                Some(t) => {
                    // non-essential = the longest cap-ascending prefix
                    // whose TOTAL upper bound stays strictly below θ
                    let mut b = 0usize;
                    while b < terms.len() && prefix[b + 1] < t {
                        b += 1;
                    }
                    b
                }
            };
            if e >= terms.len() {
                break; // even ALL terms together cannot beat θ
            }
            // next candidate doc: min current doc among essential terms
            let mut next_doc: Option<u32> = None;
            for (i, (_, _, rows)) in terms.iter().enumerate().skip(e) {
                if let Some(&(d, _)) = rows.get(cursors[i]) {
                    next_doc = Some(next_doc.map_or(d, |m| m.min(d)));
                }
            }
            let Some(doc) = next_doc else { break };
            // score the essential contributions at `doc`
            let mut score = 0.0_f64;
            for (i, (_, w, rows)) in terms.iter().enumerate().skip(e) {
                if let Some(&(d, s)) = rows.get(cursors[i])
                    && d == doc
                {
                    score += s * w;
                    visited += 1;
                    cursors[i] += 1;
                }
            }
            // non-essential terms: only if the upper bound clears θ
            // (strict `<` prune — equal bounds are still evaluated)
            for (i, (_, w, rows)) in terms.iter().enumerate().take(e).rev() {
                let upper = score + prefix[i + 1];
                if th.is_some_and(|t| upper < t) {
                    break;
                }
                // seek this term's cursor to `doc` (postings doc-sorted)
                let rows_tail = &rows[cursors[i].min(rows.len())..];
                if let Ok(off) = rows_tail.binary_search_by_key(&doc, |&(d, _)| d) {
                    score += rows_tail[off].1 * w;
                    visited += 1;
                }
            }
            // insert into the sorted top (score desc · doc asc)
            let beats = |a: (u32, f64), b: (u32, f64)| {
                a.1.total_cmp(&b.1).reverse().then(a.0.cmp(&b.0)).is_lt()
            };
            if score > 0.0 {
                let pos = top
                    .iter()
                    .position(|&entry| beats((doc, score), entry))
                    .unwrap_or(top.len());
                if pos < k {
                    top.insert(pos, (doc, score));
                    top.truncate(k);
                }
            }
        }
        (
            top,
            PruneStats {
                postings_visited: visited,
                postings_dense: dense,
            },
        )
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
