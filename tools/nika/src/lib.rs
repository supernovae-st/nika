// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika — re-exports from nika-engine for backward compatibility.
//!
//! The execution engine lives in the `nika-engine` crate.
//! This re-export layer ensures existing `use nika::*` imports continue working.

pub use nika_engine::*;
