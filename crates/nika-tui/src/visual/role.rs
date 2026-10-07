// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The product palette, relocated to [`nika_tui_view::visual::role`]
//! (ADR-143), the one mapping the renderer and every viewer paint with:
//! this path keeps the renderer's callers unchanged.

pub(crate) use nika_tui_view::visual::role::surface;
pub use nika_tui_view::visual::role::{style, verb};
