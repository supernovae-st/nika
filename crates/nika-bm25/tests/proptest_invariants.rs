// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gate 6 PROPERTY · BM25 invariants verified via proptest.
#![allow(
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
// Test-scope · `.expect()` panics on bad fixtures IS the test failure ·
// `as u32` casts are bounded by proptest strategies (≤20 docs).
//!
//! Properties per Manning IIR ch.11 + Robertson 1994 ·
//! 1. Score is always finite (no NaN/Inf even on pathological input).
//! 2. Score is non-negative when there is term overlap.
//! 3. Zero query-term overlap → score = 0.0 exactly.
//! 4. Monotonicity in TF · same doc-length + same IDF, more occurrences → higher score.
//! 5. Saturation · doubling TF yields strictly less than 2× score (BM25 asymptote).

use nika_bm25::{BmIndex, BmParams, EagerIndex};
use proptest::prelude::*;

/// Strategy · vocabulary of 4-8 lowercase tokens · documents of 1-12 tokens each ·
/// corpora of 2-20 documents. Keeps the search space tractable while exercising
/// the full state machine (`add` → `finalize` → `score` → `top_k`).
fn vocab() -> impl Strategy<Value = Vec<&'static str>> {
    static VOCAB: &[&str] = &[
        "auto",
        "car",
        "insurance",
        "best",
        "fast",
        "cheap",
        "policy",
        "claim",
    ];
    Just(VOCAB.to_vec())
}

fn doc_text(vocab: Vec<&'static str>) -> impl Strategy<Value = String> {
    proptest::collection::vec(0..vocab.len(), 1..=12).prop_map(move |idxs| {
        idxs.into_iter()
            .map(|i| vocab[i])
            .collect::<Vec<_>>()
            .join(" ")
    })
}

proptest! {
    #[test]
    fn score_is_finite(corpus in proptest::collection::vec(vocab().prop_flat_map(doc_text), 2..=20),
                       query in vocab().prop_flat_map(doc_text)) {
        let mut idx = BmIndex::new(BmParams::default());
        for (i, doc) in corpus.iter().enumerate() {
            idx.add_document((i + 1) as u32, doc);
        }
        idx.finalize();
        for i in 0..corpus.len() {
            if let Some(s) = idx.score(&query, (i + 1) as u32) {
                prop_assert!(s.is_finite(), "score must be finite · doc {} · score {}", i, s);
                prop_assert!(s >= 0.0, "score must be non-negative · got {}", s);
            }
        }
    }

    #[test]
    fn zero_overlap_yields_zero(corpus in proptest::collection::vec(vocab().prop_flat_map(doc_text), 2..=10)) {
        let mut idx = BmIndex::new(BmParams::default());
        for (i, doc) in corpus.iter().enumerate() {
            idx.add_document((i + 1) as u32, doc);
        }
        idx.finalize();
        // Query with tokens guaranteed absent from vocab.
        let zero_overlap = "xyzzy plover quux";
        for i in 0..corpus.len() {
            let s = idx.score(zero_overlap, (i + 1) as u32);
            prop_assert_eq!(s, Some(0.0));
        }
    }

    #[test]
    fn top_k_descending(corpus in proptest::collection::vec(vocab().prop_flat_map(doc_text), 3..=15),
                       query in vocab().prop_flat_map(doc_text),
                       k in 1usize..=10) {
        let mut idx = BmIndex::new(BmParams::default());
        for (i, doc) in corpus.iter().enumerate() {
            idx.add_document((i + 1) as u32, doc);
        }
        idx.finalize();
        let top = idx.top_k(&query, k);
        prop_assert!(top.len() <= k);
        prop_assert!(top.len() <= corpus.len());
        for w in top.windows(2) {
            prop_assert!(w[0].1 >= w[1].1, "must be descending · got {} then {}", w[0].1, w[1].1);
        }
    }

    #[test]
    fn top_k_truncate_invariant(corpus in proptest::collection::vec(vocab().prop_flat_map(doc_text), 5..=15),
                                query in vocab().prop_flat_map(doc_text),
                                k1 in 1usize..=5,
                                k2 in 6usize..=15) {
        // Larger-k call is a superset (by score) of smaller-k call.
        let mut idx = BmIndex::new(BmParams::default());
        for (i, doc) in corpus.iter().enumerate() {
            idx.add_document((i + 1) as u32, doc);
        }
        idx.finalize();
        let small = idx.top_k(&query, k1);
        let large = idx.top_k(&query, k2);
        prop_assert!(large.len() >= small.len());
        // First k1 entries of large == small (stable sort by score then doc_id).
        for (a, b) in small.iter().zip(large.iter()) {
            prop_assert_eq!(a.0, b.0, "doc_id mismatch at truncate boundary");
            prop_assert!((a.1 - b.1).abs() < 1e-9, "score mismatch at truncate boundary");
        }
    }
}

// ─── Deterministic monotonicity test · Robertson 1994 saturation ───

#[test]
fn monotonicity_term_frequency() {
    // 2 docs of equal length · doc 1 has "auto" 1x · doc 2 has "auto" 3x (+padding).
    // Same avgdl · same IDF · doc 2 score should be strictly higher.
    let mut idx = BmIndex::new(BmParams::default());
    idx.add_document(1, "auto pad pad pad pad"); // tf(auto)=1 · len=5
    idx.add_document(2, "auto auto auto pad pad"); // tf(auto)=3 · len=5
    idx.finalize();
    let s1 = idx.score("auto", 1).expect("doc 1 must exist");
    let s2 = idx.score("auto", 2).expect("doc 2 must exist");
    assert!(s2 > s1, "more TF must yield higher score · s1={s1} s2={s2}");
    // Saturation · 3x TF must yield less than 3x score (BM25 asymptote at k1+1).
    let ratio = s2 / s1;
    assert!(s2 < 3.0 * s1, "saturation must apply · s2/s1={ratio}");
}

#[test]
fn saturation_doubling_yields_less_than_2x() {
    let mut idx = BmIndex::new(BmParams::default());
    idx.add_document(1, "auto pad pad pad"); // tf(auto)=1 · len=4
    idx.add_document(2, "auto auto pad pad"); // tf(auto)=2 · len=4
    idx.finalize();
    let s1 = idx.score("auto", 1).expect("doc 1");
    let s2 = idx.score("auto", 2).expect("doc 2");
    assert!(s2 > s1);
    let ratio = s2 / s1;
    assert!(
        s2 < 2.0 * s1,
        "BM25 saturation · s2/s1 should be <2 · got {ratio}"
    );
}

proptest! {
    /// 6. BM25S equivalence (Lù 2024 · arxiv.org/abs/2407.03618) — the
    /// eager sparse path is BYTE-IDENTICAL to the lazy dense path on
    /// positive-score documents, over the whole corpus/query space
    /// (same addition order → exact IEEE equality, not approximate).
    #[test]
    fn eager_sparse_equals_lazy_dense_exactly(
        docs in proptest::collection::vec(doc_text(
            ["auto", "car", "insurance", "best", "fast", "cheap", "policy", "claim"].to_vec()
        ), 2..20),
        query in doc_text(
            ["auto", "car", "insurance", "best", "fast", "cheap", "policy", "claim"].to_vec()
        ),
        k in 1usize..10,
    ) {
        let mut idx = BmIndex::new(BmParams::default());
        for (i, d) in docs.iter().enumerate() {
            idx.add_document(i as u32, d);
        }
        idx.finalize();
        let eager = EagerIndex::build(&idx).expect("finalized");
        let lazy: Vec<(u32, f64)> = idx
            .top_k(&query, k)
            .into_iter()
            .filter(|(_, s)| *s > 0.0)
            .collect();
        let mut fast = eager.top_k(&query, k);
        // the lazy path may fill the tail of k with zero-score docs the
        // sparse path never visits — compare on the positive prefix
        fast.truncate(lazy.len());
        prop_assert_eq!(lazy, fast);
    }
}
