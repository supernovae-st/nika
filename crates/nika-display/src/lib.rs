// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run-comprehension surface — fold (`state`) · glyphs/colour (`theme`)
//! · frames (`render`) · execution-flow reads (`flow`) · the run's fruit
//! and form-sanity reads (`fruit`) · bounded output summaries (`shape`)
//! · painted source spans (`snippet`) · the shared formatter and
//! colour-capability seam (`format`) · the glyph/hint vocabulary
//! (`vocab`) · structural chrome (`chrome` — rail · panel · bar ·
//! banner) · deterministic demo streams (`demo`).
//! One truth in, text out; no I/O lives here.
//!
//! Descended from `nika-cli/src/display` at the 15k prod-LOC wall
//! (2026-07-10 · the `nika-dap`/`nika-cap` precedent) — per
//! D-2026-07-09-N1 this is the cli UNIT in a second member, named by
//! parentage in `docs/crate-specs/nika-display.md`. `nika-cli` re-exports
//! the whole surface at its old `display::` path (zero call-site churn).

// Test code speaks expect/unwrap freely (the same stance the parent
// nika-cli crate holds at its root — inherited by the descent).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod check_journey;
mod check_laws;
mod check_models;
pub mod check_render;
pub mod chrome;
mod claims;
pub mod dag_art;
pub mod demo;
pub mod flow;
pub mod format;
pub mod fruit;
pub mod render;
pub mod shape;
pub mod snippet;
pub mod state;
pub mod theme;
pub mod vocab;
pub mod wires;
