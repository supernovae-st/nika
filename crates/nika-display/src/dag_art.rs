// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The terminal DAG art for a checked workflow — real wires when the
//! layout can be truthful, the wave listing otherwise (never a wrong
//! picture). Descended from nika-cli's `verbs::graph` + `wires`
//! (2026-07-29 · the 15k wall · the nika-dap/nika-cap precedent: the
//! drawing lives beside the theme seam it paints through). The theme
//! rides in from the caller's ONE resolution chain, so a pipe still
//! gets escape-free bytes while a TTY sees the art it was owed.

use std::fmt::Write as _;

use nika_check::CheckReport;
pub use nika_graph::{GraphDoc, project};
use nika_schema::raw::RawWorkflow;

use crate::theme::Theme;
use crate::wires::WireGraph;

/// The themed wire art for ONE checked workflow — the same map
/// `graph --format ascii` prints, exposed so sibling surfaces (`check`)
/// can show the DAG beside their verdict without re-deriving the
/// projection.
#[must_use]
pub fn ascii_art(wf: &RawWorkflow, report: &CheckReport, theme: Theme) -> String {
    to_ascii(&project(wf, report), report, theme)
}

/// The drawing over an already-projected doc: real wires when the
/// layout can be truthful, the wave listing otherwise.
#[must_use]
pub fn to_ascii(doc: &GraphDoc, report: &CheckReport, theme: Theme) -> String {
    if let Some(art) = crate::wires::render(&wire_graph(doc, &report.waves), theme) {
        return format!("{art}\n");
    }
    // Honest fallback: one row per wave — order without invented wires.
    let mut text = String::new();
    let mut cursor = 0usize;
    for (i, wave) in report.waves.iter().enumerate() {
        let ids: Vec<&str> = doc.nodes[cursor..cursor + wave.len()]
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        cursor += wave.len();
        let _ = writeln!(text, "  wave {} · {}", i + 1, ids.join(" · "));
    }
    text
}

/// Build the decoupled topology from the checked projection (node order
/// IS wave order — the same slicing inspect trusts).
#[must_use]
pub fn wire_graph(doc: &GraphDoc, waves: &[Vec<usize>]) -> WireGraph {
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
