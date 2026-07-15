// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `permits:` block — the declared capability boundary.
//!
//! **Extracted 2026-07-03 to the `nika-cap` L0 crate** — the canonical,
//! dependency-light home for the capability-boundary vocabulary (so a future
//! `nika-policy` can consume it without pulling in the parser). Re-exported
//! here at the original module path so every `nika_schema::types::permits::*`
//! / `crate::types::Permits` import resolves unchanged. See `nika-cap` for the
//! types, the fits predicate (`allows_host`/`allows_path`/`allows_tool`/…) and
//! the `union`/`intersect` algebra.

pub use nika_cap::{ExecPermit, FsPermits, NetPermits, Permits};
