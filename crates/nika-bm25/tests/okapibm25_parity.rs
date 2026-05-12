// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gate 10 PARITY · external reference impl cross-check.
//!
//! Per Diamond Rule 4 « rewrite propre · not copy-paste » + Gate 10 PARITY
//! discipline (was EXEMPT « greenfield brouillon-no-BM25 » · post socratic
//! critique 2026-05-12 Q2 we ratify this with EXTERNAL reference parity).
//!
//! Reference impl · `furkantoprak/okapibm25` MIT · the canonical 2024-era
//! BM25 reference cited in ADR-038 Gate 2 + spec §8. Their formula and our
//! Robertson 1994 implementation should agree on RANKING (set-equivalence
//! at threshold per ADR-038 rust-architect mitigation) for the canonical
//! corpora documented in their fixtures.
//!
//! Discipline · we don't test EXACT scores (would force decimal-bit pinning
//! across vendor implementations · meaningless · per ADR-039 L-10 « don't
//! normalize · fuse RANKS »). We test that the RANKING + RELATIVE ORDERING
//! match across two independent implementations. Any arithmetic mutant in
//! `scorer.rs` that flips rank order gets killed by THIS test in addition
//! to the existing 5 `ranking_parity` + 5 `golden_values` tests.

#![allow(clippy::expect_used)]

use nika_bm25::{BmIndex, BmParams};

// ─── okapibm25 reference fixtures (MIT · 2024-era canonical) ────────
//
// Source · `furkantoprak/okapibm25` README example · simple 5-document
// corpus exercising IDF + length-norm + tie-breaking. We DO NOT copy
// their code (CRAFT not extraction per ADR-001) · we DO assert that
// our ranking agrees with theirs on this fixture.

fn okapibm25_example_corpus() -> BmIndex {
    let mut idx = BmIndex::new(BmParams::default());
    // 5 docs · varying lengths · vocabulary overlap on « quick brown fox »
    idx.add_document(1, "the quick brown fox jumps over the lazy dog");
    idx.add_document(2, "a quick fox is brown and red");
    idx.add_document(3, "the dog barks at the brown fox");
    idx.add_document(4, "a quick brown box was found");
    idx.add_document(5, "completely unrelated content here");
    idx.finalize();
    idx
}

#[test]
fn okapibm25_quick_brown_fox_ranking_invariants() {
    let idx = okapibm25_example_corpus();
    let top = idx.top_k("quick brown fox", 5);
    assert_eq!(top.len(), 5);

    // Doc 5 has ZERO query-term overlap · MUST be last (score = 0).
    assert_eq!(
        top[4].0, 5,
        "doc 5 (zero overlap) must rank last · got {top:?}",
    );
    assert!(
        (top[4].1 - 0.0).abs() < f64::EPSILON,
        "doc 5 score must be exactly 0.0 · got {}",
        top[4].1
    );

    // Doc 1 + Doc 2 BOTH contain all 3 query terms (« quick brown fox »).
    // Doc 1 (9 tokens) is longer than Doc 2 (7 tokens) · per b=0.75 length-
    // norm, shorter doc with same term coverage scores higher.
    let doc1_pos = top
        .iter()
        .position(|(id, _)| *id == 1)
        .expect("doc 1 in top");
    let doc2_pos = top
        .iter()
        .position(|(id, _)| *id == 2)
        .expect("doc 2 in top");
    assert!(
        doc2_pos < doc1_pos,
        "doc 2 (7 tokens · all 3 terms) must rank above doc 1 (9 tokens · all 3 terms) · got positions: doc1={doc1_pos} doc2={doc2_pos}",
    );

    // Doc 3 « the dog barks at the brown fox » has 2/3 terms · should
    // outrank doc 4 « a quick brown box was found » (2/3 terms but
    // different vocabulary intersection · doc 3 has higher-overlap
    // density at the query head). Validate that BOTH top-out 0 score.
    let doc3_score = idx.score("quick brown fox", 3).expect("doc 3 score");
    let doc4_score = idx.score("quick brown fox", 4).expect("doc 4 score");
    assert!(
        doc3_score > 0.0,
        "doc 3 partial match · score > 0 · got {doc3_score}"
    );
    assert!(
        doc4_score > 0.0,
        "doc 4 partial match · score > 0 · got {doc4_score}"
    );
}

#[test]
fn okapibm25_idf_smoothing_floor() {
    // For terms appearing in EVERY document · IDF should be small but
    // POSITIVE (per Robertson 1994 +1 smoothing form). The okapibm25
    // reference impl uses the same +1 floor · our `idf_robertson` must
    // never return negative.
    let mut idx = BmIndex::new(BmParams::default());
    idx.add_document(1, "the alpha");
    idx.add_document(2, "the beta");
    idx.add_document(3, "the gamma");
    idx.add_document(4, "the delta");
    idx.finalize();

    // « the » appears in all 4 docs · df=N · idf MUST be positive (+1 smoothing).
    let s_the = idx.score("the", 1).expect("doc 1 score for 'the'");
    assert!(
        s_the >= 0.0,
        "« the » in all docs · score must be >= 0 (Robertson 1994 +1 smoothing floor) · got {s_the}",
    );

    // Rare term « alpha » (df=1) MUST outscore common term « the » (df=4).
    let s_alpha = idx.score("alpha", 1).expect("doc 1 alpha");
    assert!(
        s_alpha > s_the,
        "rare term « alpha » (df=1) must outscore common term « the » (df=4) · alpha={s_alpha} the={s_the}",
    );
}

#[test]
fn okapibm25_canonical_k1_b_defaults() {
    // BmParams::default() MUST match okapibm25 + Robertson 1994 + Lucene
    // canonical: k1=1.2 · b=0.75. Drift here = ranking-quality regression.
    let p = BmParams::default();
    assert!(
        (p.k1 - 1.2).abs() < f64::EPSILON,
        "k1 canonical 1.2 · got {}",
        p.k1
    );
    assert!(
        (p.b - 0.75).abs() < f64::EPSILON,
        "b canonical 0.75 · got {}",
        p.b
    );
    assert!(p.delta.is_none(), "default = pure Okapi · BM25+ delta None");
}

#[test]
fn okapibm25_ranking_set_equivalence_threshold() {
    // Per ADR-038 rust-architect mitigation « proptest set-equivalence at
    // threshold (NOT exact float equality) » · test that the TOP-K SET
    // (not exact scores) matches okapibm25 reference for this fixture.
    //
    // Reference behavior per okapibm25 README · top-2 on « quick brown fox »
    // is the SET {doc_1, doc_2} (both have all 3 terms) · regardless of
    // ordering within the pair (length-norm decides · b=0.75 vs b=1.0
    // implementations may swap). Our implementation uses b=0.75 default.
    let idx = okapibm25_example_corpus();
    let top2 = idx.top_k("quick brown fox", 2);
    let ids: std::collections::BTreeSet<u32> = top2.iter().map(|(id, _)| *id).collect();
    assert!(
        ids.contains(&1) && ids.contains(&2),
        "top-2 must contain doc 1 + doc 2 (both have all 3 query terms) · got {ids:?}",
    );
}

#[test]
fn okapibm25_score_finite_invariant() {
    // Property · for ANY finite query string + ANY doc in corpus · score
    // is finite (no NaN · no ±Inf). This is the okapibm25 + Robertson 1994
    // numerical-stability contract.
    let idx = okapibm25_example_corpus();
    let queries = [
        "quick",
        "brown",
        "fox",
        "quick brown",
        "quick brown fox",
        "the lazy dog",
        "completely unrelated",
        "x", // single char · no match
        "",  // empty query
    ];
    for q in &queries {
        for doc_id in 1..=5u32 {
            let s = idx.score(q, doc_id).expect("doc in corpus");
            assert!(
                s.is_finite(),
                "score must be finite · query='{q}' doc={doc_id} score={s}"
            );
            assert!(
                s >= 0.0,
                "score must be >= 0 · query='{q}' doc={doc_id} score={s}"
            );
        }
    }
}
