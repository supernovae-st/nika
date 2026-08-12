// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-tui-core` — the TUI session LAW in one Rust crate (T28 ·
//! D-2026-08-11-N6), compiled native (the ratatui renderer, T27) and to
//! WASM (the web studio). Three surfaces, one law — two implementations
//! of a derivation diverge always (paid twice on 2026-08-10).
//!
//! The DRAWING stays out of here: cells, effects and spans belong to the
//! renderer and to the projected YAML SSOT — a design iteration never
//! asks for a `wasm-pack build`. This crate owns what a screen may CLAIM:
//! the session model and every number the surfaces show, derived, never
//! hand-written.
//!
//! The parity proof is the derivation itself: `tests/fixtures/*.json`
//! pins (Workflow, Run, expected) triples and the crate must land on the
//! same bits.

pub mod claims;
pub mod derive;
pub mod ingress;
pub mod model;
pub mod plan;
pub mod wasm;
