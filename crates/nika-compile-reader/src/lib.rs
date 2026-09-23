// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The frozen deterministic reader of the Compile core and the typed semantic plan it
//! produces. One free intent in, one [`lexicon::Reading`] out: a private [`plan::Plan`] of
//! operations, effects, obligations, bindings, constraints and typed rules, every element
//! anchored by a verbatim excerpt of the intent. Nothing here invents an element, calls a
//! model or grants authority; `nika-compile` composes, assembles and previews what is read.
//!
//! Descended from `nika-compile` at the 15k prod-LOC wall (2026-09-21 · the ADR-137
//! precedent): per D-2026-07-09-N1 this is ONE architectural unit in TWO workspace members.
//! The dependency runs one way, `nika-compile` → `nika-compile-reader`, never back.
//!
//! The reader is FROZEN (a safety floor): no cue or head is added; every new law is a
//! structural one or lives in the typed plan. Every public type is `#[non_exhaustive]`,
//! the forward-compatibility ratchet of the boundary: a consumer constructs an element
//! through its constructor and matches with a wildcard arm, so the reader may grow a
//! variant without breaking the member above it.
//!
//! - [`lexicon`] · the head, cue and marker tables and the reading itself (`read`).
//! - [`plan`] · the typed semantic plan and its provenance record.
//! - [`rules`] · the closed rule grammar, its typed computations ([`aggregate`]) and the
//!   tokens it reads ([`rule_tokens`], [`rule_cues`], [`stages`]).
//! - [`objects`], [`gates`], [`paths`], [`columns`], [`hot`] · the structural laws: what a
//!   clause names, where a human gate sits, what a literal token is, which words are
//!   columns, and the strict HOT admission over the reader's own vocabulary.
//! - [`text`] · the text helpers the compiler and the onboarding surface share.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        // The plan is `#[non_exhaustive]`: its tests build it field by field from `Default`,
        // as every crate outside this one must.
        clippy::field_reassign_with_default
    )
)]

pub mod aggregate;
mod anchor;
pub mod candidate;
pub mod cardinality;
pub mod columns;
pub mod fidelity;
pub mod gates;
pub mod hot;
pub mod lexicon;
pub mod objects;
pub mod paths;
pub mod plan;
pub mod rule_cues;
pub mod rule_tokens;
pub mod rules;
pub mod shape;
pub mod stages;
pub mod structure;
pub mod text;
pub mod trigger_words;
pub mod unknowns;
pub mod words;
