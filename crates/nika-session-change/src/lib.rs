// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project change a session proposal writes, its review, its outcome
//! and the record of its consent (ADR-126 · ADR-133) — a size-cap member of
//! the `nika-session` unit (D-2026-07-09-N1 · ADR-144), placed below the
//! session: `nika-session → nika-session-change`, never back.
//!
//! - [`change`] — the typed change set: built once from exact bytes,
//!   consumed by BOTH the preview and the apply, witnessed against stale
//!   targets, audited by the same facade `nika check` uses, landed below
//!   the root's own descriptor.
//! - [`review`] — the factual review of a Ready candidate before any
//!   consent, read from the candidate's own bytes, the compiler's requested
//!   boundary and the set's own audit rows; no model describes a workflow.
//! - [`outcome`] — the typed answers a host renders and a remote host
//!   judges by identity: the proposal a consent names, the gate or question
//!   an answer names, the class of a refusal.
//! - [`consent`] — the append-only consent journal under the project: what
//!   was previewed, what landed, when.
//! - [`world`] — where a workflow's exact bytes reach, typed from the check's
//!   data journey: local files, a service on this machine, a connected
//!   service, or a destination the check cannot determine. A fixture or a
//!   local contract server is never shown as the real service.
//! - [`work`] — the work a session holds, typed once for every host: what
//!   the next line answers with the identity an answer names, and one
//!   serializable snapshot of the request, the candidate, the saved
//!   workflow and the last run. It grants nothing.
//!
//! The session owns the conversation, the rounds and the money gate; it
//! reaches this member downward, and keeps these modules at their
//! historical `nika_session::{change, consent, outcome, review}` paths.
//! What a run's trace proves is read here from its owner
//! (`nika_trace::run_view`), never re-derived.

#![forbid(unsafe_code)]

pub mod change;
mod closure;
pub mod consent;
pub mod decision;
pub mod draft;
pub mod outcome;
pub mod review;
pub mod save_run;
pub mod work;
pub mod world;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod change_fs_tests;
