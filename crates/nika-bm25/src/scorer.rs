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
///
/// Returns `0.0` if `tf == 0` (term absent) · `avgdl == 0` (empty corpus).
#[must_use]
#[inline]
pub(crate) fn term_score(tf: u32, doc_len: u32, avgdl: f64, idf: f64, k1: f64, b: f64) -> f64 {
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
    idf * numer / denom
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
        assert!((term_score(0, 10, 5.0, 1.5, 1.2, 0.75) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn term_score_empty_corpus_returns_zero() {
        assert!((term_score(3, 10, 0.0, 1.5, 1.2, 0.75) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn term_score_finite() {
        let s = term_score(2, 8, 6.0, 1.2, 1.2, 0.75);
        assert!(s.is_finite());
        assert!(s > 0.0);
    }

    #[test]
    fn term_score_saturation() {
        // Doubling tf increases score but less than 2x (saturation).
        let avgdl = 6.0;
        let idf = 1.2;
        let s1 = term_score(1, 6, avgdl, idf, 1.2, 0.75);
        let s2 = term_score(2, 6, avgdl, idf, 1.2, 0.75);
        assert!(s2 > s1);
        assert!(s2 < 2.0 * s1);
    }
}
