// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-bm25-kernel` — `MemoryRecall` trait adapter for `nika-bm25-core`.
//!
//! Q6 split (Option D per rust-architect audit 2026-05-12) ·
//! `nika-bm25-core` ships the pure BM25 algorithm (publishable standalone
//! on crates.io · zero `nika-kernel` dep) · `nika-bm25-kernel` (this crate)
//! ships the ~80 LOC adapter that implements `nika_kernel::MemoryRecall`
//! for `nika_bm25_core::BmIndex` (intra-doc links activate at W3 GREEN
//! phase when types are publicly exported).
//!
//! **Status** · W3 admission target · ADR-038 + ADR-043 binding plans ·
//! see `docs/crate-specs/nika-bm25-core.md` for the 12-gate readiness map.

// Adapter impl lands at W3 GREEN phase. Pre-W3 scaffold is intentionally
// empty · `cargo check` passes via the `pub fn _placeholder` trick below
// (will be deleted on admission).
#[doc(hidden)]
pub fn _placeholder_w3() {}
