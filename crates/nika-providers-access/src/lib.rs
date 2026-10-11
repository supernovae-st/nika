// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # nika-providers-access — how this machine reaches an intelligence
//!
//! A size-cap member of the nika-providers unit (L1.5 · D-2026-07-09-N1 · the ADR-122 member
//! note). It holds what a run's access is judged from, as values:
//!
//! - [`probe`] — the facts a provider probe states: the readiness ladder, the execution locus,
//!   a key's auth rung and an opt-in model listing;
//! - [`resolve_access`] — the access plan resolved over those facts, with its witnesses;
//! - [`route_identity`] — the identity of the route a call takes and its durable projections;
//! - [`canonical_provider`] — the catalogue's canonical provider id.
//!
//! No I/O and no runtime: `nika-providers` probes, dispatches and admits, and re-exports every
//! item here at its historical path (`nika_providers::{probe, resolve_access, route_identity}`,
//! `nika_providers::profile::canonical_provider`). The edge is lateral and never goes back.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod probe;
pub mod resolve_access;
pub mod route_identity;

/// Catalog aliases (`grok` → `xai`, `claude` → `anthropic`) resolve to
/// the canonical provider id this binary seeds. Unknown names pass
/// through so the unknown-provider arm still names the raw prefix.
#[must_use]
pub fn canonical_provider(id: &str) -> &str {
    nika_catalog::find_provider(id).map_or(id, |row| row.id)
}
