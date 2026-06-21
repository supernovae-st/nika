// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-cli` — the operator surface of the engine (L4 · ADMITTED 2026-06-21).
//!
//! Seeded 2026-06-11 with the **display fold**: `RunView` (a pure fold over
//! the real [`nika_event::Event`] stream) + `frame` rendering (spec
//! `docs/crate-specs/nika-cli.md` §3) + the deterministic demo storyboards.
//! The first-15-min verb suite (D-2026-06-10-N6) is here; the law every
//! addition obeys: **render = pure function of the event stream** — terminal,
//! `--json`, SSE and the DAG webview are four views of one truth.
//!
//! Status: ADMITTED — all 12 gates (crate-spec §11). The crate is `nika-cli`
//! but the binary's public name is `nika` (clap `#[command(name = "nika")]`);
//! the release renames the artifact to `nika`. The L5 composition root will
//! later own that name as a <500-LOC wrapper over this surface.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod demo;
pub mod display;
pub mod verbs;

pub use display::render::{frame, verdict_frame};
pub use display::state::{RunView, TaskRow, TaskState};
pub use display::theme::{Role, Theme};
