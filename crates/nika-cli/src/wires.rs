// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The thin bridge from the CLI's checked projection to the display
//! crate's wire drawing — [`nika_display::wires`] owns the geometry,
//! this module only reshapes [`GraphDoc`] + wave indices into the
//! decoupled [`WireGraph`] the drawing takes.

use crate::verbs::graph::GraphDoc;
pub(crate) use nika_cli_display_wires::{WireGraph, render_with};
use nika_display::wires as nika_cli_display_wires;

use crate::display::theme::Theme;

/// Build the decoupled topology from the checked projection (node
/// order IS wave order — the same slicing inspect trusts).
pub(crate) fn wire_graph(doc: &GraphDoc, waves: &[Vec<usize>]) -> WireGraph {
    let mut out: Vec<Vec<(String, String)>> = Vec::with_capacity(waves.len());
    let mut cursor = 0usize;
    for wave in waves {
        out.push(
            doc.nodes[cursor..cursor + wave.len()]
                .iter()
                .map(|n| (n.id.clone(), n.verb.to_owned()))
                .collect(),
        );
        cursor += wave.len();
    }
    WireGraph {
        waves: out,
        edges: doc
            .edges
            .iter()
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect(),
    }
}

/// The historical entry — the same drawing `graph --format ascii`
/// always spoke, now routed through the display crate.
pub(crate) fn render(doc: &GraphDoc, waves: &[Vec<usize>], theme: Theme) -> Option<String> {
    nika_cli_display_wires::render(&wire_graph(doc, waves), theme)
}
