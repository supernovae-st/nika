// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where each region of the workspace screen stands, relocated to
//! [`nika_tui_view::workspace::geometry`] (ADR-143): this path keeps the
//! renderer's callers unchanged.

pub(crate) use nika_tui_view::workspace::geometry::Separator;
#[cfg(test)]
pub(crate) use nika_tui_view::workspace::geometry::{
    ASIDE_MIN, CONVERSATION_MIN_ROWS, CONVERSATION_MIN_WIDTH, OBJECT_MIN_ROWS,
};
pub use nika_tui_view::workspace::geometry::{
    ASIDE_MIN_WIDTH, Arrangement, Geometry, Layout, MIN_SIZE, SIDE_BY_SIDE_MIN_WIDTH,
    TALL_HEADER_MIN_HEIGHT, fits,
};
