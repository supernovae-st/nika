// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native session — bare `nika` on a terminal (ADR-125 · One Door ·
//! wave 4).
//!
//! A grounded conversation over the INSTALLED engine, never a temporary
//! workflow: the runtime observes the project ([`snapshot`]), selects the
//! intelligence the human chose ([`intelligence`] · a seat, an API, a
//! local engine, or none), hands the reasoner a minimal typed bundle
//! through the [`broker`] (the proven root · only what the human named ·
//! secrets redacted · the environment never injected · provenance kept),
//! answers Nika facts from the engine's own authorities ([`facts`]), and
//! reads every reply through the [`guard`] before a human sees it — a
//! named builtin, model, code, MCP server, verb or field this engine does
//! not carry is corrected, never presented as real.
//!
//! What the session must NOT own is what it queries: the grammar, the
//! catalogs, the codes, the checker, the runtime. Its identity core
//! ([`identity`]) says so to the model in six laws.

pub mod broker;
pub mod facts;
pub mod guard;
pub mod identity;
pub mod intelligence;
pub mod reasoner;
pub mod runtime;
pub mod snapshot;

pub use broker::{ContextBroker, SessionContextBundle, Snippet};
pub use guard::{Finding, KnownWorld};
pub use intelligence::{
    DataLocus, IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence,
    UserIntelligencePreference,
};
pub use reasoner::{ReasonError, Reply, ScriptedReasoner, SessionReasoner};
pub use runtime::{SessionRuntime, TurnOutcome};
pub use snapshot::ProjectSnapshot;
