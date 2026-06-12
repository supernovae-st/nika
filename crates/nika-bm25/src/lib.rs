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
//! - Lù 2024 *BM25S* · arxiv.org/abs/2407.03618 · the eager sparse
//!   scoring architecture behind [`EagerIndex`]

mod eager;
mod index;
mod query;
mod scorer;
mod tokenize;

// MemoryRecall trait adapter deferred to W4-W10 per ADR-078 step 5+6 ·
// the full `MemoryHit` shape requires metadata the BM25 satellite alone
// cannot populate (content · level · tags) · L2 `RecallPool` orchestrator
// merges with metadata from sister satellites.

pub use eager::EagerIndex;
pub use index::BmIndex;
pub use tokenize::tokenize as tokenize_text;

/// Tunable BM25 parameters.
///
/// Use [`Self::new`] for explicit values · [`Default`] for canonical
/// (k1=1.2 · b=0.75 per Robertson 1994).
///
/// - `k1` controls term-frequency saturation. Typical: 1.2.
/// - `b` controls length-normalization (`0..=1`). Typical: 0.75.
/// - `delta` reserved for BM25+ lower-bound smoothing (Lv & Zhai 2011 ·
///   adds `δ` constant fixing « long doc with one rare-term hit »
///   under-scoring). `None` = canonical Okapi (current default).
///   `Some(0.5..=1.0)` activates BM25+ when shipped W4+ (consumer-signal
///   gated per LOCK-031 · per rust-ml audit 2026-05-12 Q1).
///
/// # Score normalization
///
/// Scores returned by [`BmIndex::score`] / [`BmIndex::top_k`] are
/// **raw f64** in approximately `[0, |Q| · (k1+1)]`. Per 2026 SOTA hybrid
/// retrieval consensus (Cormack 2009 RRF · Mem0 weighted-RRF · Microsoft
/// Azure AI Search · `ParadeDB`) · **do NOT normalize · fuse RANKS not
/// SCORES**. The L2 `nika-memory::RecallPool` (W10) consumes the
/// sorted output and derives ranks for `nika-rrf` fusion · raw score is
/// preserved for tiebreaking + logging.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct BmParams {
    /// Term-frequency saturation tunable (canonical 1.2).
    pub k1: f64,
    /// Length-normalization tunable in `0..=1` (canonical 0.75).
    pub b: f64,
    /// BM25+ lower-bound smoothing constant (Lv & Zhai 2011) · reserved
    /// for W4+ activation · `None` = pure Okapi (current canonical).
    pub delta: Option<f64>,
}

impl BmParams {
    /// Create new params (Okapi canonical · no BM25+ smoothing).
    ///
    /// Caller responsible for sanity (`k1 > 0` · `b ∈ 0..=1`).
    #[must_use]
    pub const fn new(k1: f64, b: f64) -> Self {
        Self { k1, b, delta: None }
    }

    /// Create new BM25+ params with lower-bound smoothing constant.
    ///
    /// `delta` typical range `0.5..=1.0` per Lv & Zhai 2011. RESERVED
    /// for W4+ activation · current `BmIndex` ignores this field
    /// (Okapi formula only · BM25+ wire-up gated on consumer signal).
    #[must_use]
    pub const fn with_delta(k1: f64, b: f64, delta: f64) -> Self {
        Self {
            k1,
            b,
            delta: Some(delta),
        }
    }
}

impl Default for BmParams {
    fn default() -> Self {
        Self::new(1.2, 0.75)
    }
}
