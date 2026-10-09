// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A task or run state as glyph and role, relocated to
//! [`nika_tui_view::visual::state`] (ADR-143): this path keeps the
//! renderer's callers unchanged.

pub use nika_tui_view::visual::state::{STATES, cell};
