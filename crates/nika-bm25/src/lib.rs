// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-bm25` — BM25 (Okapi) lexical scoring satellite for the Nika
//! diamond memory subsystem.
//!
//! **Status** · W3 admission target · ADR-038 binding plan ·
//! `docs/crate-specs/nika-bm25.md` for the 12-gate readiness map.
//!
//! Pure-algo · zero I/O at trait boundary · zero ML deps · pairs with
//! `nika-hnsw` (W7) + `nika-rrf` (W4) for hybrid lexical+dense retrieval.
//!
//! # Example (target API at admission · today partial)
//!
//! ```text
//! use nika_bm25::{BmIndex, BmParams};
//!
//! let mut idx = BmIndex::new(BmParams::default());
//! idx.add_document(1, "the cat sat on the mat");
//! idx.add_document(2, "the dog ran in the park");
//! idx.finalize();
//!
//! let scores = idx.top_k("cat mat", 5);
//! ```
//!
//! # References
//! - Robertson & Walker 1994 · canonical Okapi BM25
//! - Manning · Raghavan · Schütze 2008 *IIR* ch. 11

// BM25 tunable parameters (Robertson 1994 canonical).
//!
//! - `k1` controls term-frequency saturation. Typical: 1.2.
//! - `b` controls length-normalization (`0` = none · `1` = full). Typical: 0.75.

/// Tunable BM25 parameters.
///
/// Use [`Self::new`] for explicit values · [`Default`] for canonical
/// (k1=1.2 · b=0.75).
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct BmParams {
    pub k1: f64,
    pub b: f64,
}

impl BmParams {
    /// Create new params. Caller responsible for sanity (`k1 > 0` · `b ∈ [0..=1]`).
    #[must_use]
    pub const fn new(k1: f64, b: f64) -> Self {
        Self { k1, b }
    }
}

impl Default for BmParams {
    fn default() -> Self {
        Self::new(1.2, 0.75)
    }
}
