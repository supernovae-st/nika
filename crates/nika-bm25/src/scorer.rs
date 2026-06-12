// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! BM25 scoring primitives · Robertson 1994 canonical.
//!
//! Single-document score for one query term:
//!
//! ```text
//!                              tf * (k1 + 1)
//! score(t, d) = idf(t) · ─────────────────────────────────────
//!                        tf + k1 * (1 - b + b * |d| / avgdl)
//! ```
//!
//! IDF (probabilistic · with floor at 0 to avoid negative weights on
//! very common terms):
//!
//! ```text
//! idf(t) = ln((N - df + 0.5) / (df + 0.5) + 1)
//! ```
//!
//! Cohérent `okapibm25` MIT reference + Manning IIR ch.11.

/// Probabilistic IDF with `+1` smoothing inside the log (the standard
/// Lucene / Elasticsearch / okapibm25 form). Guarantees `idf >= 0` even
/// for terms appearing in every document.
///
/// Returns `0.0` if `n == 0` or `df > n` (defensive · empty corpus).
#[must_use]
#[inline]
pub(crate) fn idf_robertson(n: usize, df: usize) -> f64 {
    if n == 0 || df > n {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let n_f = n as f64;
    #[allow(clippy::cast_precision_loss)]
    let df_f = df as f64;
    ((n_f - df_f + 0.5) / (df_f + 0.5) + 1.0).ln()
}

/// Per-term BM25 score contribution.
///
/// - `tf` · term frequency in the document
/// - `doc_len` · document length in tokens
/// - `avgdl` · average document length in the corpus
/// - `idf` · pre-computed term idf
/// - `k1`/`b` · canonical tunables (1.2 / 0.75)
/// - `delta` · BM25+ lower-bound smoothing (Lv & Zhai 2011 ·
///   *Lower-Bounding Term Frequency Normalization* · CIKM; the variant
///   table in Lù 2024 BM25S · arxiv.org/abs/2407.03618 §2 is the
///   arXiv-accessible formula source): for a PRESENT term the score
///   becomes `idf · (saturation + δ)`, so a matching document is
///   bounded BELOW by `δ·idf` no matter how long it is — fixing the
///   Okapi pathology where one rare-term hit in a very long document
///   scores arbitrarily close to a non-matching document. Absent terms
///   still contribute exactly `0.0` (δ never applies).
///
/// Returns `0.0` if `tf == 0` (term absent) · `avgdl == 0` (empty corpus).
#[must_use]
#[inline]
pub(crate) fn term_score(
    tf: u32,
    doc_len: u32,
    avgdl: f64,
    idf: f64,
    k1: f64,
    b: f64,
    delta: Option<f64>,
) -> f64 {
    if tf == 0 || avgdl <= 0.0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let tf_f = f64::from(tf);
    #[allow(clippy::cast_precision_loss)]
    let len_f = f64::from(doc_len);
    let norm = 1.0 - b + b * (len_f / avgdl);
    let numer = tf_f * (k1 + 1.0);
    let denom = tf_f + k1 * norm;
    if denom <= 0.0 {
        return 0.0;
    }
    idf * (numer / denom + delta.unwrap_or(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idf_zero_corpus() {
        assert!((idf_robertson(0, 0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn idf_all_docs_contain_term() {
        // df == n → ln((0.5 / (n+0.5)) + 1) > 0 (with +1 smoothing form)
        let idf = idf_robertson(4, 4);
        assert!(idf > 0.0);
        assert!(idf.is_finite());
    }

    #[test]
    fn idf_rare_term_higher_than_common() {
        let rare = idf_robertson(100, 1);
        let common = idf_robertson(100, 50);
        assert!(rare > common);
    }

    #[test]
    fn term_score_tf_zero_returns_zero() {
        assert!((term_score(0, 10, 5.0, 1.5, 1.2, 0.75, None) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn term_score_empty_corpus_returns_zero() {
        assert!((term_score(3, 10, 0.0, 1.5, 1.2, 0.75, None) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn term_score_finite() {
        let s = term_score(2, 8, 6.0, 1.2, 1.2, 0.75, None);
        assert!(s.is_finite());
        assert!(s > 0.0);
    }

    #[test]
    fn term_score_saturation() {
        // Doubling tf increases score but less than 2x (saturation).
        let avgdl = 6.0;
        let idf = 1.2;
        let s1 = term_score(1, 6, avgdl, idf, 1.2, 0.75, None);
        let s2 = term_score(2, 6, avgdl, idf, 1.2, 0.75, None);
        assert!(s2 > s1);
        assert!(s2 < 2.0 * s1);
    }

    // ── Gate 6 PROPERTY: the BM25 invariants proven over the input space,
    // not just on the hand-picked rows above. These are the contract a
    // scorer MUST hold for ranking to mean anything (Robertson 1994 §; IIR
    // ch.11) — a regression in the formula breaks one of them.
    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

        /// IDF is finite and NON-NEGATIVE for every well-formed (n, df) with
        /// 1 ≤ df ≤ n — the `+1`-inside-log smoothing guarantee. A negative
        /// weight (the classic Robertson-Sparck-Jones footgun on terms in
        /// >half the corpus) would let a common term DROP a document's rank.
        #[test]
        fn prop_idf_non_negative_and_finite(n in 1usize..2_000_000, df_frac in 0.0f64..=1.0) {
            // df spans the full 1..=n range via the fraction.
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let df = ((df_frac * (n as f64 - 1.0)) as usize + 1).min(n);
            let idf = idf_robertson(n, df);
            proptest::prop_assert!(idf.is_finite(), "idf({n},{df}) = {idf} not finite");
            proptest::prop_assert!(idf >= 0.0, "idf({n},{df}) = {idf} is negative");
        }

        /// IDF is monotonically NON-INCREASING in df: a rarer term (lower df)
        /// weighs at least as much as a commoner one. This IS the "rare terms
        /// discriminate" semantic at the heart of BM25 ranking.
        #[test]
        fn prop_idf_monotone_in_df(n in 1usize..1_000_000, a in 1usize..1_000_000, b in 1usize..1_000_000) {
            let df1 = a.min(b).min(n);
            let df2 = a.max(b).min(n);
            // df1 ≤ df2 ≤ n by construction.
            let rarer = idf_robertson(n, df1);
            let commoner = idf_robertson(n, df2);
            proptest::prop_assert!(
                rarer + 1e-9 >= commoner,
                "idf must not increase with df: idf({n},{df1})={rarer} < idf({n},{df2})={commoner}"
            );
        }

        /// A present term (tf ≥ 1) with a real corpus (avgdl > 0) and a valid
        /// idf always scores finite and ≥ 0 — no NaN/∞ can leak into the
        /// ranking sum (which `top_k` sorts with `total_cmp`).
        #[test]
        fn prop_term_score_finite_non_negative(
            tf in 1u32..1_000_000,
            doc_len in 0u32..2_000_000,
            avgdl in 0.5f64..1_000_000.0,
            idf in 0.0f64..50.0,
        ) {
            let s = term_score(tf, doc_len, avgdl, idf, 1.2, 0.75, None);
            proptest::prop_assert!(s.is_finite(), "score {s} not finite");
            proptest::prop_assert!(s >= 0.0, "score {s} negative");
        }

        /// TF monotonicity: more occurrences never LOWER the score (BM25 is
        /// non-decreasing in tf · strictly increasing when idf > 0, equal at
        /// idf = 0). A regression here would make a doc rank worse for
        /// containing the query term MORE often.
        #[test]
        fn prop_term_score_monotone_in_tf(
            tf in 1u32..100_000,
            bump in 1u32..100_000,
            doc_len in 1u32..1_000_000,
            avgdl in 0.5f64..100_000.0,
            idf in 0.0f64..50.0,
        ) {
            let lo = term_score(tf, doc_len, avgdl, idf, 1.2, 0.75, None);
            let hi = term_score(tf.saturating_add(bump), doc_len, avgdl, idf, 1.2, 0.75, None);
            proptest::prop_assert!(hi + 1e-9 >= lo, "score dropped as tf rose: {lo} -> {hi}");
        }

        /// Length normalization: with b > 0 a LONGER document scores no
        /// higher than a shorter one for the same term frequency — the
        /// penalty that stops verbose docs from dominating. Equal only when
        /// idf = 0 (everything is 0).
        #[test]
        fn prop_term_score_longer_doc_penalized(
            tf in 1u32..10_000,
            short_len in 1u32..500_000,
            extra in 1u32..500_000,
            avgdl in 0.5f64..100_000.0,
            idf in 0.0f64..50.0,
        ) {
            let short = term_score(tf, short_len, avgdl, idf, 1.2, 0.75, None);
            let long = term_score(tf, short_len.saturating_add(extra), avgdl, idf, 1.2, 0.75, None);
            proptest::prop_assert!(long <= short + 1e-9, "longer doc scored higher: {short} -> {long}");
        }
    }
}

#[cfg(test)]
mod bm25_plus_tests {
    use super::*;

    #[test]
    fn delta_lower_bounds_matching_terms_and_never_touches_absent() {
        let idf = 1.5;
        let delta = Some(1.0);
        // absent term: delta NEVER applies — exactly 0.0
        assert!((term_score(0, 10_000, 5.0, idf, 1.2, 0.75, delta) - 0.0).abs() < f64::EPSILON);
        // a PRESENT term in a pathologically long document: Okapi decays
        // toward 0; BM25+ is bounded below by δ·idf
        let okapi = term_score(1, 1_000_000, 5.0, idf, 1.2, 0.75, None);
        let plus = term_score(1, 1_000_000, 5.0, idf, 1.2, 0.75, delta);
        assert!(okapi < 0.01, "okapi decays: {okapi}");
        assert!(plus >= idf * 1.0, "bm25+ lower bound: {plus}");
        // and on a normal document, delta adds EXACTLY δ·idf
        let base = term_score(2, 8, 6.0, idf, 1.2, 0.75, None);
        let plussed = term_score(2, 8, 6.0, idf, 1.2, 0.75, delta);
        assert!((plussed - (base + idf)).abs() < 1e-12);
    }

    #[test]
    fn none_delta_is_byte_identical_to_okapi() {
        for (tf, len) in [(1u32, 6u32), (3, 12), (7, 2)] {
            let a = term_score(tf, len, 6.0, 1.3, 1.2, 0.75, None);
            let b = term_score(tf, len, 6.0, 1.3, 1.2, 0.75, Some(0.0));
            assert!((a - b).abs() < f64::EPSILON, "δ=0 ≡ None");
        }
    }
}
