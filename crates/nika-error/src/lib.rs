// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-error` — canonical error infrastructure for the Nika diamond.
//!
//! Design: **Option C+** (trait-based error hierarchy).
//!
//! - [`NikaErrorCode`] trait — implemented by per-crate error enums
//! - [`NikaError`] wrapper — `Box<dyn NikaErrorCode>`, the unified type
//! - [`CoreError`] enum — cross-cutting errors (`Validation`, `NotFound`, `Internal`)
//! - [`NikaCode`] struct — dual wire ("NIKA-140") + typed (num, category, slug)
//!
//! Foundation value types (IDs, Cost, `TrustLevel`, etc.) live in `nika-types`
//! and are re-exported here for backward compatibility.
//!
//! See `BRAINSTORM_PHASE1_DECISIONS.md` §D2 for rationale.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
    )
)]

pub mod codes;
pub mod core_error;
pub mod nika_error;
pub mod traits;

// ─── Backward-compatible re-exports from nika-types ─────────────────
// All value types that were originally in this crate now live in
// nika-types. Re-exporting both modules and prelude items ensures
// zero breaking change for existing consumers.
pub use nika_types::baggage;
pub use nika_types::budget;
pub use nika_types::cancel;
pub use nika_types::checkpoint;
pub use nika_types::compression;
pub use nika_types::cost;
pub use nika_types::hash;
pub use nika_types::id;
pub use nika_types::memory;
pub use nika_types::resource;
pub use nika_types::retry;
pub use nika_types::role;
pub use nika_types::schema;
pub use nika_types::token_usage;
pub use nika_types::trust;

/// Convenience re-exports for common usage.
///
/// ```rust
/// use nika_error::prelude::*;
/// ```
pub mod prelude {
    pub use crate::codes::{self, Category, NikaCode, Severity};
    pub use crate::core_error::CoreError;
    pub use crate::nika_error::{NikaError, NikaResult};
    pub use crate::traits::NikaErrorCode;

    // Re-export all nika-types prelude items
    pub use nika_types::prelude::*;
}

pub use prelude::*;
