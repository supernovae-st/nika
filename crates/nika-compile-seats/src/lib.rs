// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The two capabilities a host lends a preparation of the Compile core, and the reasoning
//! record their calls share. Neither grants authority: a seat answers one closed choice the
//! compiler revalidates, a room runs a candidate where no effect escapes.
//!
//! - [`decide`] · the bounded decision seats (the WARM strategy): one closed choice among
//!   options Nika already found admissible, or NONE; a provider seated through a JSON-schema
//!   enum; the answer revalidated before use and projected for provenance.
//! - [`compose`] · the admissible options themselves: the distinct private plans the COLD
//!   proposals give, each judged by a deterministic feasibility filter against the reading
//!   before any seat sees them, with the dimensions the recalled candidates suggest.
//! - [`rehearse`] · the rehearsal port: a host that runs a candidate in a safe room built from
//!   the observed world says what the run did, and the report maps to the behavioural judge's
//!   run.
//! - [`reasoning`] · the reasoning one call is asked for and the record of what it reported.
//! - [`judge`] · the untrusted state a judging seat reads: the request as compiled and as
//!   first stated, its answers, the observed world, the candidate's bytes, and a revision's
//!   change beside the request of the base it revises.
//! - [`repairs`] · the laws that end the verifier's work: its repairs when no repair count
//!   bounds them, and its questions on bytes a judge already rejected.
//! - [`objects`] · the JSON objects of a seat's text: its one answer, competing answers, the
//!   group a syntax diagnostic targets.
//! - [`shelf`] · the references an authoring seat reads (the embedded recall, the callable
//!   contracts), rendered and receipted.
//! - [`foundry`] · recalled Foundry knowledge qualified by a decision seat against the request
//!   before an author reads it, and the record of what was found, shown and discarded; an
//!   admitted release's executable components resolved, bound, expanded into the document and
//!   checked, with the reuse witness re-derived from the candidate's bytes; and the operations a
//!   document revision states over a complete base, applied with their record.
//! - [`remote`] · what a door that holds no project admits from its caller's engine to prepare
//!   as `nika compile` does: the observation of the stated files and the trial inputs a room is
//!   built from.
//! - [`authority`] · the authority over a seat's requests: the bound a door states, the counters
//!   that enforce it, and the account a receipt keeps (descended from the seats' doors, ADR-152).
//!
//! A size-cap member of the `nika-onboard` unit (ADR-146 · D-2026-07-09-N1 · the ADR-144
//! precedent), placed below the seats' doors: `nika-compile-cognition` →
//! `nika-compile-seats` → `nika-compile`, never back. The seats' doors keep the modules at
//! their historical paths, `nika_compile_cognition::{authority, decide, rehearse}`, and the
//! onboarding surface re-exports them from there.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod authority;
pub mod compose;
pub mod decide;
pub mod foundry;
pub mod judge;
pub mod objects;
pub mod reasoning;
pub mod rehearse;
pub mod remote;
pub mod repairs;
pub mod shelf;
