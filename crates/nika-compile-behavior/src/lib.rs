// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The behavioural contract a request states, independent of any candidate, and its typed
//! judgment over what a round of rehearsals consumed and wrote. Pure over the request's reading
//! and the evidence a host hands over: nothing here reads a candidate document, runs a
//! rehearsal, touches the file system, calls a model or grants authority.
//!
//! - [`behavior`] · the contract a request states, the judgment of one round, the selection
//!   among candidates and the canonical readings they compare by: exact numbers, the
//!   `nika:convert` CSV reading, values bound to the sha256 of the bytes read.
//! - [`instant_shape`] · the form and offset of a date-time text, which the judge reads before
//!   it lets text order stand for time order.
//!
//! A size-cap member of the `nika-onboard` unit (ADR-149 · D-2026-07-09-N1 · the ADR-146
//! precedent), placed below the candidate laws: `nika-compile-fidelity` →
//! `nika-compile-behavior` → `nika-compile-reader`, never back. Both descended from
//! `nika-compile-fidelity` at the 15k prod-LOC wall, which keeps them at their historical
//! paths, `nika_compile_fidelity::behavior` and `nika_compile_fidelity::fidelity::instant_shape`.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        // The plan is `#[non_exhaustive]`: its tests build it field by field from `Default`,
        // as every crate outside the reader must.
        clippy::field_reassign_with_default
    )
)]

pub mod behavior;
mod instants;

pub use instants::instant_shape;
