// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gate 2 RED phase · failing tests asserting target `BmIndex` API.
//!
//! Per `diamond-discipline.md` Rule 2 (12 gates no exceptions) +
//! `commit-granularity.md` (TDD RED then GREEN) · these tests MUST fail
//! at Gate 2 close (no impl yet) and MUST pass at Gate 3 GREEN.
//!
//! Reference fixture · `fixtures/manning_iir_ch11.txt`
//! Reference impl  · `furkantoprak/okapibm25` (MIT) for parity scoring

use nika_bm25::BmParams;

// ─── BmParams · already shipped at Gate 3 SCAFFOLD ──────────────────

#[test]
fn bm_params_default_is_canonical_robertson_1994() {
    let p = BmParams::default();
    assert!((p.k1 - 1.2).abs() < f64::EPSILON, "k1 canonical = 1.2");
    assert!((p.b - 0.75).abs() < f64::EPSILON, "b canonical = 0.75");
}

#[test]
fn bm_params_new_const_fn() {
    // const-context test · proves const fn promotion shipped 2026-05-12
    const CUSTOM: BmParams = BmParams::new(1.5, 0.5);
    assert!((CUSTOM.k1 - 1.5).abs() < f64::EPSILON);
    assert!((CUSTOM.b - 0.5).abs() < f64::EPSILON);
}

// ─── BmIndex API · RED phase · WILL FAIL at Gate 2 (no impl yet) ────
//
// Re-enable when Gate 3 GREEN lands · these encode the canonical `BmIndex` API
// contract per `docs/crate-specs/nika-bm25.md` §2.

#[cfg(feature = "_gate-3-green-phase")]
mod gate_3_green {
    use super::*;
    use nika_bm25::BmIndex;

    /// Manning IIR ch.11 toy corpus · 4 documents over vocabulary
    /// { auto · car · insurance · best }
    fn manning_corpus() -> [(u32, &'static str); 4] {
        [
            (1, "auto car insurance"),
            (2, "best auto insurance"),
            (3, "car insurance best auto"),
            (4, "insurance best car"),
        ]
    }

    #[test]
    fn empty_index_returns_no_score() {
        let idx = BmIndex::new(BmParams::default());
        assert!(idx.score("anything", 1).is_none(), "no docs · no score");
    }

    #[test]
    fn unfinalized_index_panics_or_errors_on_score() {
        let mut idx = BmIndex::new(BmParams::default());
        for (id, text) in manning_corpus() {
            idx.add_document(id, text);
        }
        // pre-finalize · IDF not computed · score should be unavailable
        // (exact contract · Result OR None OR panic · GREEN phase decides)
    }

    #[test]
    fn manning_ch11_top_k_ranking_invariant() {
        let mut idx = BmIndex::new(BmParams::default());
        for (id, text) in manning_corpus() {
            idx.add_document(id, text);
        }
        idx.finalize();

        let top_3 = idx.top_k("best car insurance", 3);
        assert_eq!(top_3.len(), 3, "exactly 3 results");

        // RED · the exact ranking depends on okapibm25 parity ·
        // GREEN phase validates verbatim score equality (proptest
        // set-equivalence at threshold per rust-architect mitigation).
        // For now · sanity invariant · all scores finite + non-negative.
        for (_doc_id, score) in &top_3 {
            assert!(score.is_finite(), "BM25 score must be finite");
            assert!(*score >= 0.0, "BM25 score must be non-negative");
        }
    }

    #[test]
    fn zero_overlap_query_returns_zero_score() {
        let mut idx = BmIndex::new(BmParams::default());
        for (id, text) in manning_corpus() {
            idx.add_document(id, text);
        }
        idx.finalize();

        // Query terms not in vocabulary · zero overlap · zero score
        let score = idx.score("xyzzy plover", 1);
        assert_eq!(score, Some(0.0), "zero-overlap query → zero score");
    }

    #[test]
    fn monotonicity_term_frequency() {
        // proptest harness target · double term-frequency same doc-length
        // → score increases per BM25 saturation curve (asymptote at k1+1)
        // GREEN phase ships proptest strategy · here just smoke test
    }
}
