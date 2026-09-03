// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's LAWS — what a run obeys before and after it executes, none of
//! which dispatches a task (ADR-127 · the nika-runtime size-cap member
//! split): the one-voice error, the typed contracts, the public record
//! mirror, the input origins, the engine identity, the record integrity,
//! the secret custody, the sandbox verdict and the event stamp seams. `nika-runtime` re-exports every item
//! at its historical path; this crate is the `nika-runtime` unit's second
//! member, never a new architectural unit.

#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "../../nika-runtime/build_support.rs"]
mod build_support;
pub mod compat_record;
pub mod contract;
pub mod errors;
pub mod identity;
pub mod integrity;
pub mod origins;
pub mod resume_fields;
pub mod sandbox_select;
pub mod secret;
pub mod stamp;
pub mod witness;

pub use nika_dataflow::{expr, jq, record};
pub use stamp::{DeterministicStamper, EventSink, Stamper, SystemStamper, VecSink};
