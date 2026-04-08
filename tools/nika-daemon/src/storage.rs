// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Re-export storage types from the `nika-storage` crate.
//!
//! The storage layer was extracted into its own crate so that `nika-serve`
//! and other binaries can embed job persistence without pulling in the
//! full daemon dependency tree. This module re-exports everything so that
//! existing `use crate::storage::*` imports continue to work unchanged.

pub use nika_storage::*;
