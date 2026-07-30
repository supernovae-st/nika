// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The per-rung claim sentences the check verdict renders — extracted
//! from `check_render.rs` 2026-07-30 (the 1500-LOC cap): the render
//! stays, the claim text descends.

use nika_check::CheckReport;

/// The TYPES claim, narrowed to exactly what the lane covers (F3 ·
/// 2026-07-30). With no blind spot the old sentence stands; with one,
/// the line names the count and up to three of the refs the run will
/// judge — a ✔ that claims exactly what it checked, never more.
pub(crate) fn types_claim(report: &CheckReport) -> String {
    let refs: std::collections::BTreeSet<&str> = report
        .unverifiable_output_refs
        .iter()
        .map(|r| r.reference.as_str())
        .collect();
    if refs.is_empty() {
        return "deep references fit the shapes tasks declare · builtin output has none".to_owned();
    }
    let n = refs.len();
    let shown: Vec<&str> = refs.iter().take(3).copied().collect();
    let more = if n > 3 {
        format!(" · +{} more", n - 3)
    } else {
        String::new()
    };
    let noun = if n == 1 { "ref" } else { "refs" };
    format!(
        "deep references fit the shapes tasks declare · {n} {noun} into unshaped task \
         outputs are unverifiable — the run judges them ({}{more})",
        shown.join(" · ")
    )
}
