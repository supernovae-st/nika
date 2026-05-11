// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Inverted index for BM25 lexical scoring.
//!
//! - `add_document(id, text)` · per-doc term-frequency table
//! - `finalize()` · compute avgdl + IDF per term (Robertson 1994 canonical)
//! - `score(query, doc_id)` · BM25 single-doc score
//! - `top_k(query, k)` · ranked top-k retrieval
//!
//! Discipline · `std::collections::BTreeMap` only · deterministic
//! iteration · zero `parking_lot` / `dashmap` dep at L0 surface (the
//! L2 `RecallPool` per ADR-078 handles cross-thread sharding).

use crate::BmParams;
use crate::scorer::idf_robertson;
use crate::tokenize::tokenize;
use std::collections::BTreeMap;

/// Per-document state · term-frequency table + doc length.
#[derive(Debug, Clone)]
struct DocEntry {
    /// term → frequency in this document
    tf: BTreeMap<String, u32>,
    /// total token count (∑ tf values)
    len: u32,
}

/// BM25 inverted index.
///
/// Construct with [`BmIndex::new`] · add docs with [`Self::add_document`] ·
/// call [`Self::finalize`] before [`Self::score`] / [`Self::top_k`].
///
/// Per ADR-078 the recall surface uses `&self` after finalize · interior
/// state is read-only post-finalization.
#[derive(Debug, Clone)]
pub struct BmIndex {
    params: BmParams,
    docs: BTreeMap<u32, DocEntry>,
    /// term → number of documents containing the term (df)
    df: BTreeMap<String, u32>,
    /// term → idf(term) · populated by [`Self::finalize`]
    idf: BTreeMap<String, f64>,
    /// Average document length across the corpus · populated by [`Self::finalize`].
    avgdl: f64,
    /// `true` once [`Self::finalize`] has run.
    finalized: bool,
}

impl BmIndex {
    /// Create an empty index with the given parameters.
    #[must_use]
    pub fn new(params: BmParams) -> Self {
        Self {
            params,
            docs: BTreeMap::new(),
            df: BTreeMap::new(),
            idf: BTreeMap::new(),
            avgdl: 0.0,
            finalized: false,
        }
    }

    /// Number of documents in the index.
    #[must_use]
    pub fn doc_count(&self) -> usize {
        self.docs.len()
    }

    /// `true` iff [`Self::finalize`] has been called.
    #[must_use]
    pub fn is_finalized(&self) -> bool {
        self.finalized
    }

    /// Add a document. Re-adding the same `id` overwrites.
    ///
    /// Calling after [`Self::finalize`] re-opens the index for mutation
    /// (callers MUST `finalize` again before scoring).
    pub fn add_document(&mut self, id: u32, text: &str) {
        let tokens = tokenize(text);
        let mut tf: BTreeMap<String, u32> = BTreeMap::new();
        for t in tokens {
            *tf.entry(t).or_insert(0) += 1;
        }
        let len = tf.values().sum::<u32>();

        // If overwriting, decrement df for terms that disappear.
        if let Some(old) = self.docs.remove(&id) {
            for term in old.tf.keys() {
                if let Some(c) = self.df.get_mut(term) {
                    *c = c.saturating_sub(1);
                    if *c == 0 {
                        self.df.remove(term);
                    }
                }
            }
        }

        // Increment df for each unique term in the new doc.
        for term in tf.keys() {
            *self.df.entry(term.clone()).or_insert(0) += 1;
        }

        self.docs.insert(id, DocEntry { tf, len });
        self.finalized = false;
    }

    /// Compute IDF + avgdl over the current corpus.
    pub fn finalize(&mut self) {
        let n = self.docs.len();
        if n == 0 {
            self.avgdl = 0.0;
            self.idf.clear();
            self.finalized = true;
            return;
        }

        let total_len: u64 = self.docs.values().map(|d| u64::from(d.len)).sum();
        // `n` is `usize` from BTreeMap::len · realistic corpora fit in f64.
        // Per zero-unwrap doctrine + Karpathy 800 LOC clarity, use cast intentionally.
        #[allow(clippy::cast_precision_loss)]
        let n_f = n as f64;
        #[allow(clippy::cast_precision_loss)]
        let total_f = total_len as f64;
        self.avgdl = total_f / n_f;

        self.idf.clear();
        for (term, df) in &self.df {
            self.idf
                .insert(term.clone(), idf_robertson(n, *df as usize));
        }

        self.finalized = true;
    }

    /// Internal accessor · params.
    pub(crate) fn params(&self) -> BmParams {
        self.params
    }

    /// Internal accessor · avgdl.
    pub(crate) fn avgdl(&self) -> f64 {
        self.avgdl
    }

    /// Internal accessor · IDF for a term (returns 0.0 if absent).
    pub(crate) fn term_idf(&self, term: &str) -> f64 {
        self.idf.get(term).copied().unwrap_or(0.0)
    }

    /// Internal accessor · iterate `(doc_id, tf, doc_len)` for scoring.
    pub(crate) fn docs_iter(&self) -> impl Iterator<Item = (u32, &BTreeMap<String, u32>, u32)> {
        self.docs.iter().map(|(id, d)| (*id, &d.tf, d.len))
    }

    /// Internal accessor · single doc lookup.
    pub(crate) fn doc(&self, id: u32) -> Option<(&BTreeMap<String, u32>, u32)> {
        self.docs.get(&id).map(|d| (&d.tf, d.len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_index() {
        let idx = BmIndex::new(BmParams::default());
        assert_eq!(idx.doc_count(), 0);
        assert!(!idx.is_finalized());
    }

    #[test]
    fn add_then_finalize() {
        let mut idx = BmIndex::new(BmParams::default());
        idx.add_document(1, "auto car insurance");
        idx.add_document(2, "best auto insurance");
        assert_eq!(idx.doc_count(), 2);
        assert!(!idx.is_finalized());
        idx.finalize();
        assert!(idx.is_finalized());
        assert!((idx.avgdl() - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn overwrite_doc_updates_df() {
        let mut idx = BmIndex::new(BmParams::default());
        idx.add_document(1, "auto car");
        idx.add_document(1, "totally new tokens"); // overwrite
        idx.finalize();
        assert_eq!(idx.doc_count(), 1);
        // auto / car should have been decremented to 0 (removed).
        assert!(idx.term_idf("auto") == 0.0);
        assert!(idx.term_idf("car") == 0.0);
        assert!(idx.term_idf("totally") > 0.0);
    }

    #[test]
    fn add_invalidates_finalized() {
        let mut idx = BmIndex::new(BmParams::default());
        idx.add_document(1, "auto");
        idx.finalize();
        assert!(idx.is_finalized());
        idx.add_document(2, "car");
        assert!(!idx.is_finalized());
    }
}
