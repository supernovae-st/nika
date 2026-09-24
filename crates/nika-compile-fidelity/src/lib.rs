// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The laws a candidate `.nika` document is judged by, and the two forms that tie a
//! candidate to the plan: the constrained sketch a seat proposes and the plan a candidate
//! states by its structure. Pure over (request · plan · projected document): nothing here
//! reads an intent, calls a model or grants authority; `nika-compile` runs the laws at every
//! door and its seats' doors state and judge their candidates through them.
//!
//! Ascended from `nika-compile-reader` at the 15k prod-LOC wall (ADR-141 · the ADR-137 and
//! ADR-138 precedents): per D-2026-07-09-N1 this is ONE architectural unit in several
//! workspace members. The dependency runs `nika-compile` → `nika-compile-fidelity` →
//! `nika-compile-reader`, never back. The reader's modules are bound at the paths the moved
//! files have always used, so the laws read the plan exactly as they did inside the reader.
//!
//! - [`fidelity`] · the laws, one structured diagnostic per refusal a seat can repair from.
//! - [`sketch`] · the constrained intermediate a seat proposes (tasks, edges, gates, stated
//!   paths and hosts), its structural laws, its typed holes and the document it states.
//! - [`candidate`] · the plan a candidate document states, and a revision's delta.

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

pub mod candidate;
pub mod fidelity;
pub mod sketch;

// The reader's plan, HOT vocabulary and lexicon at their historical module paths
// (`crate::plan`, `super::hot::fold`, `crate::lexicon::GATE_WITHOUT_EFFECT`).
use nika_compile_reader::{hot, lexicon, plan};
