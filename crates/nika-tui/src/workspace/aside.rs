// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project aside, relocated to [`nika_tui_view::workspace::aside`]
//! (ADR-143): this path keeps the renderer's callers unchanged.

pub use nika_tui_view::workspace::aside::{Aside, Entry, Tab, Verdict, lines, lines_selecting};
pub(crate) use nika_tui_view::workspace::aside::{entry_at, lines_anchored};
