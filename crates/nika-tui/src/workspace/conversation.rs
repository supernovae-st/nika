// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation panel's own rows, relocated to
//! [`nika_tui_view::workspace::conversation`] (ADR-143): this path keeps
//! the renderer's callers unchanged.

pub(crate) use nika_tui_view::workspace::conversation::invitation;
pub use nika_tui_view::workspace::conversation::{Thread, context, placeholder, title};
