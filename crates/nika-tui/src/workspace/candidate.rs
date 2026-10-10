// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate the conversation proposes, relocated with its review card
//! model to [`nika_tui_view::workspace::candidate`] (ADR-143): this path keeps
//! the renderer's callers unchanged.

pub use nika_tui_view::workspace::candidate::Proposed;
pub(crate) use nika_tui_view::workspace::candidate::{RunAfter, admitted};

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
