// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! RESEARCH CONFORMANCE — every arXiv-grounded claim in this crate,
//! verified as an executable property at scale (the « did we actually
//! implement the paper? » suite · one test per CITATIONS.md entry).
#![allow(clippy::expect_used, clippy::cast_possible_truncation)]

use nika_bm25::{BmIndex, BmParams, EagerIndex};

/// A deterministic 200-doc corpus over a 12-word vocabulary with a
/// Zipf-ish skew (term 0 everywhere · term 11 rare) and varying doc
/// lengths — big enough that lazy/eager/pruned divergences show.
fn big_corpus(params: BmParams) -> BmIndex {
    const VOCAB: [&str; 12] = [
        "auto",
        "car",
        "insurance",
        "best",
        "fast",
        "cheap",
        "policy",
        "claim",
        "quote",
        "premium",
        "deductible",
        "actuary",
    ];
    let mut idx = BmIndex::new(params);
    for i in 0u32..200 {
        let mut words: Vec<&str> = Vec::new();
        for (j, w) in VOCAB.iter().enumerate() {
            // skew: term j appears in docs where i % (j+1) == 0, with
            // frequency varying by doc — deterministic, no RNG
            if (i as usize).is_multiple_of(j + 1) {
                let reps = 1 + ((i as usize) + j) % 4;
                words.extend(std::iter::repeat_n(w, reps));
            }
        }
        // length variation: pad some docs heavily with a common word
        if i.is_multiple_of(7) {
            words.extend(std::iter::repeat_n(&"auto", 30));
        }
        idx.add_document(i, &words.join(" "));
    }
    idx.finalize();
    idx
}

const QUERIES: [&str; 8] = [
    "best car insurance",
    "cheap premium quote",
    "actuary deductible",
    "auto auto policy",
    "claim fast",
    "insurance",
    "quote premium deductible actuary",
    "zzz missing only",
];

/// Lù 2024 (arXiv:2407.03618) — BM25S eager sparse scoring is
/// BYTE-IDENTICAL to lazy dense scoring, at scale.
#[test]
fn bm25s_eager_equals_lazy_at_scale() {
    let idx = big_corpus(BmParams::default());
    let eager = EagerIndex::build(&idx).expect("finalized");
    for q in QUERIES {
        for k in [1, 3, 10, 50] {
            let lazy: Vec<(u32, f64)> = idx
                .top_k(q, k)
                .into_iter()
                .filter(|(_, s)| *s > 0.0)
                .collect();
            let mut fast = eager.top_k(q, k);
            fast.truncate(lazy.len());
            assert_eq!(lazy, fast, "BM25S divergence on {q:?} k={k}");
        }
    }
}

/// Turtle & Flood 1995 lineage (arXiv anchor: Qiao et al. 2023,
/// arXiv:2305.01203) — `MaxScore` pruning is EXACT (same top-k incl.
/// tiebreaks) and actually PRUNES (visits strictly fewer postings on
/// the skewed corpus).
#[test]
fn maxscore_is_exact_and_actually_prunes() {
    let idx = big_corpus(BmParams::default());
    let eager = EagerIndex::build(&idx).expect("finalized");
    let mut pruned_total = 0usize;
    let mut dense_total = 0usize;
    for q in QUERIES {
        for k in [1, 3, 10] {
            let dense = eager.top_k(q, k);
            let (pruned, stats) = eager.top_k_pruned_stats(q, k);
            // RANK-exact (same docs, same order); scores agree to 1e-9
            // relative — the term reorder makes byte-equality
            // impossible by design (see the method doc)
            let dense_docs: Vec<u32> = dense.iter().map(|(d, _)| *d).collect();
            let pruned_docs: Vec<u32> = pruned.iter().map(|(d, _)| *d).collect();
            assert_eq!(dense_docs, pruned_docs, "rank divergence on {q:?} k={k}");
            for ((_, a), (_, b)) in dense.iter().zip(&pruned) {
                let rel = (a - b).abs() / a.abs().max(1e-300);
                assert!(rel < 1e-9, "score drift {rel} on {q:?}");
            }
            assert!(
                stats.postings_visited <= stats.postings_dense,
                "visited must never exceed dense"
            );
            pruned_total += stats.postings_visited;
            dense_total += stats.postings_dense;
        }
    }
    assert!(
        pruned_total < dense_total,
        "across the suite, pruning must actually fire: {pruned_total} vs {dense_total}"
    );
}

/// Lv & Zhai 2011 (formula source: Lù 2024 §2) — BM25+ lower-bounds
/// every matching term by δ·idf: ANY document containing a query term
/// outranks every document containing none, regardless of length.
#[test]
fn bm25_plus_lower_bound_holds_at_scale() {
    let idx = big_corpus(BmParams::with_delta(1.2, 0.75, 1.0));
    // doc 0 contains every term; the rare term hits few docs
    let hits = idx.top_k("actuary", 200);
    let positive: Vec<u32> = hits
        .iter()
        .filter(|(_, s)| *s > 0.0)
        .map(|(d, _)| *d)
        .collect();
    // every positive-score doc REALLY contains the term (i % 12 == 0)
    for d in &positive {
        assert!(
            (*d as usize).is_multiple_of(12),
            "doc {d} scored positive without containing the term"
        );
    }
    // and every containing doc scores positive (the δ lower bound —
    // even the length-30-padded ones)
    let expected: Vec<u32> = (0u32..200)
        .filter(|i| (*i as usize).is_multiple_of(12))
        .collect();
    assert_eq!(
        positive.len(),
        expected.len(),
        "δ guarantees every match surfaces"
    );
}

/// Robertson & Walker 1994 — with delta None the engine is canonical
/// Okapi: the golden suite pins exact values elsewhere; HERE we pin
/// the cross-implementation invariant that BM25+ never reorders
/// matching docs relative to Okapi (δ adds a constant per matching
/// term — order within same-match-count docs is preserved).
#[test]
fn bm25_plus_preserves_okapi_order_within_same_match_counts() {
    let okapi = big_corpus(BmParams::default());
    let plus = big_corpus(BmParams::with_delta(1.2, 0.75, 1.0));
    // single-term query: every matching doc gains exactly δ·idf —
    // the ORDER of matching docs must be identical
    let a: Vec<u32> = okapi
        .top_k("premium", 200)
        .into_iter()
        .filter(|(_, s)| *s > 0.0)
        .map(|(d, _)| d)
        .collect();
    let b: Vec<u32> = plus
        .top_k("premium", 200)
        .into_iter()
        .filter(|(_, s)| *s > 0.0)
        .map(|(d, _)| d)
        .collect();
    assert_eq!(
        a, b,
        "a constant shift must not reorder single-term matches"
    );
}
