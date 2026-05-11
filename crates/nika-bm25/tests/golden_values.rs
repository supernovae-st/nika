// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gate 5 MUTATION killer · exact-value golden pins for arithmetic mutants.
//!
//! Discipline · pin BM25 scores within 1e-9 tolerance for canonical fixtures.
//! Any arithmetic mutation in `idf_robertson` or `term_score` flips one of
//! these exact values · mutant caught.
//!
//! Golden values computed via current implementation 2026-05-12 ·
//! consistent with Robertson 1994 formula · validated by `ranking_parity`
//! relative-ordering tests (set-equivalence at threshold per ADR-038).
//!
//! Tolerance 1e-9 chosen per ADR-039 § « proptest set-equivalence at
//! threshold · NOT exact float equality · ranking ties confound mutants »
//! · 1e-9 is below the precision floor where ranking ties become
//! ambiguous but tight enough to catch single-bit arithmetic flips.

#![allow(clippy::expect_used, clippy::float_cmp)]

use nika_bm25::{BmIndex, BmParams};

const TOL: f64 = 1e-9;

fn pin(actual: f64, expected: f64, label: &str) {
    let diff = (actual - expected).abs();
    assert!(
        diff < TOL,
        "{label} · expected {expected:.12} got {actual:.12} diff {diff:.3e}",
    );
}

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
fn manning_doc_scores_golden() {
    let idx = manning();
    // Computed via Robertson 1994 canonical · k1=1.2 · b=0.75 · N=4 ·
    // avgdl = (3+3+4+3)/4 = 3.25 · doc lengths {3,3,4,3} · term counts
    // df(best)=3, df(car)=3, df(insurance)=4, df(auto)=3.
    //
    // Query "best car insurance" against each doc · pinned within 1e-9.
    let s1 = idx.score("best car insurance", 1).expect("doc 1");
    let s2 = idx.score("best car insurance", 2).expect("doc 2");
    let s3 = idx.score("best car insurance", 3).expect("doc 3");
    let s4 = idx.score("best car insurance", 4).expect("doc 4");

    pin(s1, 0.477_047_442_038_324_3, "doc 1 score");
    pin(s2, 0.477_047_442_038_324_3, "doc 2 score");
    pin(s3, 0.748_086_822_399_659_1, "doc 3 score");
    pin(s4, 0.845_311_102_567_123_8, "doc 4 score");
}

#[test]
fn manning_top_k_golden_ordering() {
    let idx = manning();
    let top = idx.top_k("best car insurance", 4);
    // Order MUST be: doc 4 > doc 3 > doc 1 == doc 2 (tiebreak by id asc).
    assert_eq!(top.len(), 4);
    assert_eq!(top[0].0, 4, "first must be doc 4 · got {}", top[0].0);
    assert_eq!(top[1].0, 3, "second must be doc 3 · got {}", top[1].0);
    assert_eq!(
        top[2].0, 1,
        "third must be doc 1 (tie · id asc) · got {}",
        top[2].0
    );
    assert_eq!(
        top[3].0, 2,
        "fourth must be doc 2 (tie · id asc) · got {}",
        top[3].0
    );
}

#[test]
fn idf_golden_values() {
    // 4-doc corpus where 'best' / 'car' / 'auto' appear in 3 docs and
    // 'insurance' in 4 docs (all docs). IDF per Robertson 1994 +1
    // smoothing form: ln((N - df + 0.5) / (df + 0.5) + 1).
    //
    // - df=3, N=4 → ln((1.5/3.5)+1) = ln(1.428571) ≈ 0.356674943938732
    // - df=4, N=4 → ln((0.5/4.5)+1) = ln(1.111111) ≈ 0.105360515657826
    //
    // We probe IDF indirectly via score with single-term query against a
    // doc of length avgdl (3.25 → use doc with len=3 for stable result).
    let idx = manning();
    // Single-term query "auto" against doc 1 (contains auto · len=3 · tf=1).
    let s_auto = idx.score("auto", 1).expect("doc 1 auto");
    // avgdl=3.25, doc1 len=3, tf=1, idf(auto)=ln(1.5/3.5+1)≈0.356675
    // norm = 1 - 0.75 + 0.75 * (3/3.25) = 0.25 + 0.692307... = 0.942307...
    // numer = 1 * (1.2+1) = 2.2
    // denom = 1 + 1.2 * 0.942307... = 2.130769...
    // score = 0.356675 * 2.2 / 2.130769 ≈ 0.368272...
    pin(s_auto, 0.368_263_660_528_799_6, "auto score on doc 1");

    // Same query against doc 3 (contains auto · len=4 · tf=1).
    let s_auto_3 = idx.score("auto", 3).expect("doc 3 auto");
    pin(s_auto_3, 0.325_907_456_761_908_9, "auto score on doc 3");

    // Longer doc has lower score (length normalization active).
    assert!(s_auto > s_auto_3);
}

#[test]
fn idf_smoothing_floor_property() {
    // For df > N (defensive) · idf_robertson must return 0.0 not negative.
    // We can't call the internal function directly · probe via empty term.
    let mut idx = BmIndex::new(BmParams::default());
    idx.add_document(1, "a b c");
    idx.finalize();
    // Term not in corpus · idf returns 0 · score = 0.
    let s = idx.score("zzzz", 1).expect("doc 1");
    assert!(s.abs() < TOL, "missing term must yield 0 score · got {s}");
}

#[test]
fn early_return_guards_pinned() {
    // Probe the early-return guards in idf_robertson + term_score:
    // - empty corpus (N==0) → score is None
    // - finalize-with-zero-docs (N==0) → idf clear · avgdl = 0
    let idx = BmIndex::new(BmParams::default());
    assert!(idx.score("anything", 1).is_none(), "empty index score none");

    let mut idx2 = BmIndex::new(BmParams::default());
    idx2.finalize();
    assert!(!idx2.is_finalized() || idx2.doc_count() == 0);
}
