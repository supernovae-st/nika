// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-cli` — the operator surface of the engine (L4 · WIP seed).
//!
//! Seeded 2026-06-11 with the **display fold**: `RunView` (a pure fold over
//! the real [`nika_event::Event`] stream) + `frame` rendering (spec
//! `docs/crate-specs/nika-cli.md` §3) + the deterministic demo storyboards.
//! The full first-15-min verb suite (D-2026-06-10-N6) grows here at the S6
//! build; the law every addition obeys: **render = pure function of the
//! event stream** — terminal, `--json`, SSE and the DAG webview are four
//! views of one truth.
//!
//! Status: `workspace.metadata.diamond.wip` (pre-admission · no 12-gate
//! claim). The `nika-cli` bin is the dev seed; the user-facing `nika`
//! binary name stays reserved for the L5 composition root.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod demo;
pub mod display;
pub mod verbs;

pub use display::render::frame;
pub use display::state::{RunView, TaskRow, TaskState};
pub use display::theme::{Role, Theme};
