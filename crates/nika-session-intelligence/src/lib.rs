// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The intelligence a native session reasons, routes and authors with (ADR-125) — a size-cap
//! member of the `nika-session` unit (D-2026-07-09-N1 · ADR-150), placed below the session:
//! `nika-session → nika-session-intelligence`, never back.
//!
//! - [`intelligence`] — which reasoning path the human chose (an AI app they already have · an
//!   API · a local engine · none), the census of what this machine can serve now, and the
//!   resolution that refuses a choice it cannot serve with its fix, never replaces it; each
//!   path names where the project context goes.
//! - [`reasoner`] — ONE inference over the selected intelligence, never a temporary workflow:
//!   a harness seat, the provider registry and the one-shot infer verb, a scripted stand-in,
//!   or none; with the provider plane's transport the authoring seat shares.
//! - [`turn`] — the semantic act of one free line: a bounded routing decision through the same
//!   intelligence, never a lexicon and never a consent.
//! - [`authoring`] — the session's door to the ONE compiler: the seat the human's choice
//!   permits, the authoring context pinned when the session opened, and the round (the answers
//!   by stable key, the replayed plan, the questions still open).
//!
//! The session owns the conversation, the runtime, the durability of its rounds and the money
//! gate; it reaches this member downward and keeps these modules at their historical
//! `nika_session::{authoring, intelligence, reasoner, turn}` paths.

#![forbid(unsafe_code)]

pub mod authoring;
pub mod intelligence;
pub mod reasoner;
pub mod turn;
