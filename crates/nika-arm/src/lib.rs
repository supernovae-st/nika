// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ARM custody for every Nika interface.
//!
//! `nika-cadence` decides pure scheduling and ledger semantics. This L4
//! library owns the descriptor-rooted `.nika/arm/` state and the one firing
//! transaction that holds a beat lock from decision through terminal receipt.
//! Interfaces inject time, waiting, and execution; none may reinterpret the
//! journal or scan for a trace independently.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod fire;
pub mod state;

pub use fire::{
    ExecutionRunSeam, FireCtx, FireCtxError, FireVerdict, RunSeam, RunShot, RunUpshot, Wait,
    WaitSeam, fire_beat, labels,
};
pub use state::{
    ArmState, Claim, ExecutionLink, FireKind, Folded, HealOutcome, HistoryEntry, LastRecord,
    Receipt, RecordOutcome, Rotation, Unsettled,
};
