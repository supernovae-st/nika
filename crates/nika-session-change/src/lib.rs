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
//!
//! The session owns the conversation, the rounds and the money gate; it
//! reaches this member downward, and keeps these modules at their
//! historical `nika_session::{change, consent, outcome, review}` paths.
//! What a run's trace proves is read here from its owner
//! (`nika_trace::run_view`), never re-derived.

#![forbid(unsafe_code)]

pub mod change;
pub mod consent;
pub mod outcome;
pub mod review;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod change_fs_tests;
