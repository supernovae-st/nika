// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The thin bridge from the CLI's checked projection to the display
//! crate's wire drawing — [`nika_display::wires`] owns the geometry,
//! [`nika_display::dag_art`] owns the projection reshape (descended
//! 2026-07-29), this module only routes the historical call shape.

use crate::verbs::graph::GraphDoc;

use crate::display::theme::Theme;

/// The projection reshape (descended to `nika_display::dag_art`
/// 2026-07-29) + the display crate's custom-shape drawing — re-exported
/// here so the crate's call sites keep reading `crate::wires::…`.
pub(crate) use nika_display::dag_art::wire_graph;
pub(crate) use nika_display::wires::render_with;

/// The historical entry — the same drawing `graph --format ascii`
/// always spoke, now routed through the display crate.
pub(crate) fn render(doc: &GraphDoc, waves: &[Vec<usize>], theme: Theme) -> Option<String> {
    nika_display::wires::render(&wire_graph(doc, waves), theme)
}
