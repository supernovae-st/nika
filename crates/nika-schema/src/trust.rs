// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Trust system for Nika Shield — compile-time data provenance tracking.
//!
//! The plane DESCENDED to [`nika_types::trust`] at the C2 flag-day (the
//! 15k prod-LOC wall · zero in-tree consumers, so the seed lives beside
//! [`TrustLevel`] itself). This shim keeps the `nika_schema::trust`
//! paths byte-stable for any consumer — zero surface change.

// Re-export for convenience — consumers can use `nika_schema::trust::TrustLevel`.
pub use nika_error::TrustLevel;
pub use nika_types::trust::{InvocationSource, builtin_output_trust, is_categorized_builtin};
