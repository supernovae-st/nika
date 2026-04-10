// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared test helpers for the 5 builtin file tools.
//!
//! S12.P3 — the 8 S11 Shield regression tests across write/edit/grep/glob
//! all duplicated a 6-line nested-scope dance around `tool.call(args)` to
//! set up `CURRENT_TASK_TRUST` + `CURRENT_TASK_ELEVATED`. This helper
//! collapses that to a single `run_as(trust, elevated, fut).await` line
//! so future regression tests stay short and obvious.

#![cfg(test)]

use nika_core::trust::TrustLevel;
use nika_kernel::task_local::{CURRENT_TASK_ELEVATED, CURRENT_TASK_TRUST};
use std::future::Future;

/// Run `fut` with the given trust level + elevated flag set in the
/// `task_local!` cells that the Shield reads.
///
/// Both cells are always scoped, so tests do not need to stack nested
/// `.scope(...)` calls manually. Any default that the production code
/// relies on (e.g. `current_is_tainted()` returning `false` when
/// elevated is `true` regardless of trust) is therefore exercised
/// exactly like the runtime would set it.
pub(crate) async fn run_as<F, T>(trust: TrustLevel, elevated: bool, fut: F) -> T
where
    F: Future<Output = T>,
{
    CURRENT_TASK_TRUST
        .scope(trust, async move {
            CURRENT_TASK_ELEVATED.scope(elevated, fut).await
        })
        .await
}
