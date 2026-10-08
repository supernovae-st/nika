// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native Session over a wire (ADR-133's follow-up « the session over a wire »): one host
//! adapter around the actual [`nika_session::SessionRuntime`], behind two doors that speak the
//! same contract, [`wire::CONTRACT`].
//!
//! - [`host`] keeps the runtime on one worker thread and everything a client reads or decides
//!   in custody: the snapshots it published with the very `Waiting` each showed, an idempotent
//!   ledger of commands, a Stop linearized with publication, and the event log.
//! - [`machine`] is the native door, `nika session --json`: NDJSON over stdio.
//! - [`http`] is the HTTP door of `nika serve`, under `/v1/sessions`, after the server's own
//!   authentication and body limit.
//! - [`run`] is the run port a door lends its Session: the Session requests a run as data and
//!   the door executes it, or says it did not.
//! - [`open`] opens the Session as bare `nika` does.
//!
//! Nothing here reads what a line means, keeps a second state machine, compiles, or rebuilds a
//! question's identity from the wire: the Session does all of that, once, for every door.

pub mod host;
pub mod http;
pub mod machine;
pub mod open;
pub mod run;
pub mod wire;

pub use host::{Dispatch, SessionHost};
pub use wire::{CAPABILITY, CONTRACT, Command, Frame, Refused, SELECTION_CAPABILITY};
