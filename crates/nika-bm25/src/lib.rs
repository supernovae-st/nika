// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-bm25` — BM25 (Okapi) lexical scoring satellite for the Nika
//! diamond memory subsystem.
//!
//! **Status** · W3 admission target · Gate 3 GREEN shipped 2026-05-12 ·
//! ADR-038 binding plan · `docs/crate-specs/nika-bm25.md` for the
//! 12-gate readiness map.
//!
//! Pure-algo · zero I/O at trait boundary · zero ML deps · pairs with
//! `nika-hnsw` (W7) + `nika-rrf` (W4) for hybrid lexical+dense retrieval.
//!
//! # Example
//!
//! ```
//! use nika_bm25::{BmIndex, BmParams};
//!
//! let mut idx = BmIndex::new(BmParams::default());
//! idx.add_document(1, "auto car insurance");
//! idx.add_document(2, "best auto insurance");
//! idx.add_document(3, "car insurance best auto");
//! idx.add_document(4, "insurance best car");
//! idx.finalize();
//!
//! let top = idx.top_k("best car insurance", 3);
//! assert_eq!(top.len(), 3);
//! for (_doc_id, score) in &top {
//!     assert!(score.is_finite());
//!     assert!(*score >= 0.0);
//! }
//! ```
//!
//! # References
//! - Robertson & Walker 1994 · canonical Okapi BM25
//! - Manning · Raghavan · Schütze 2008 *IIR* ch. 11

mod index;
mod query;
mod scorer;
mod tokenize;

// MemoryRecall trait adapter deferred to W4-W10 per ADR-078 step 5+6 ·
// the full `MemoryHit` shape requires metadata the BM25 satellite alone
// cannot populate (content · level · tags) · L2 `RecallPool` orchestrator
// merges with metadata from sister satellites.

pub use index::BmIndex;
pub use tokenize::tokenize as tokenize_text;

/// Tunable BM25 parameters.
///
/// Use [`Self::new`] for explicit values · [`Default`] for canonical
/// (k1=1.2 · b=0.75 per Robertson 1994).
///
/// - `k1` controls term-frequency saturation. Typical: 1.2.
/// - `b` controls length-normalization (`0..=1`). Typical: 0.75.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct BmParams {
    /// Term-frequency saturation tunable (canonical 1.2).
    pub k1: f64,
    /// Length-normalization tunable in `0..=1` (canonical 0.75).
    pub b: f64,
}

impl BmParams {
    /// Create new params. Caller responsible for sanity (`k1 > 0` · `b ∈ 0..=1`).
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
