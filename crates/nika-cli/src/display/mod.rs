// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The display module — fold (`state`) · glyphs/colour (`theme`) · frames
//! (`render`) · execution-flow reads (`flow`) · bounded output summaries
//! (`shape`) · the shared formatter + colour-capability seam (`format`).
//! One truth in, text out; no I/O lives here.

pub mod flow;
pub mod format;
pub mod render;
pub mod shape;
pub mod state;
pub mod theme;
pub(crate) mod vocab;
