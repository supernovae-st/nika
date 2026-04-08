// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider Selector — Data types only
//!
//! Only `VerifyStatus` is kept (used by `verification.rs` and `app/lifecycle.rs`).

/// Connection verification status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerifyStatus {
    /// Not yet verified
    #[default]
    Unknown,
    /// Verification in progress
    Verifying,
    /// Successfully verified
    Verified,
    /// Verification failed
    Failed,
}
