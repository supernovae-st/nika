// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared pure helpers usable by any crate in the diamond (L0).
//!
//! These helpers have no I/O, no async, no external service deps. They
//! live in `nika-core` so L0 through L5 callers can all reach them without
//! importing an effect crate.

pub mod redact;

pub use redact::{redact_secrets, register_secrets};
