// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Owned-byte execution admission shared by every Nika interface.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod error;
mod service;
mod snapshot;

pub use error::ExecutionError;
pub use service::{
    AdmittedExecution, ExecutionContext, ExecutionService, ExecutionSession, ExecutionVerdict,
};
pub use snapshot::{
    CapturedUnit, ExecutionSnapshot, SNAPSHOT_FORMAT_VERSION, SnapshotLimits, SnapshotUnitKind,
};

#[cfg(test)]
mod tests;
