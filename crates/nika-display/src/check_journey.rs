// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The JOURNEY rung of `nika check` — the data voyage made readable
//! (P0-18 · CORE-18): class + counts, the named secret flows, and one
//! dim disclosure row per external endpoint (locus · retention ·
//! training in the provider's own sourced words). Split from
//! `check_render.rs` at the 1500-line file wall — same seams, no
//! behavior change.

use std::fmt::Write as _;

use nika_check::CheckReport;

use crate::check_render::mark;
use crate::theme::{Role, Theme};

/// The SECRETS headline names observed flows without re-judging consent.
/// Findings own that verdict; JOURNEY owns the complete visible projection.
pub(crate) fn secret_flow_summary(report: &CheckReport) -> String {
    let declared_flows = report
        .data_journey
        .secrets_used
        .iter()
        .map(|secret| secret.flows_to.len())
        .sum::<usize>();
    if declared_flows == 0 {
        "no declared secret reaches an effect · model echo untracked".to_owned()
    } else {
        format!(
            "{} shown in JOURNEY · consent findings above · model echo untracked",
            crate::vocab::count(declared_flows, "declared-secret flow")
        )
    }
}

/// JOURNEY rung (P0-18 · audit UX 2026-07-30) — the data voyage made
/// visible BEFORE the run: the derived class and the counts, then one ⚠
/// row per secret reaching an external destination, NAMED. Advisory by
/// design — the blocking refusal of an UNSANCTIONED flow lives in the
/// SECRETS lane (the IFC leak finding); a sanctioned flow still has to
/// be SEEN, so the row asks the operator to review consent before the
/// run. Names and classes only — never a value (law 13).
pub(crate) fn journey_rung(out: &mut String, report: &CheckReport, t: Theme) {
    let j = &report.data_journey;
    let flows: Vec<(&str, &str)> = j
        .secrets_used
        .iter()
        .flat_map(|s| {
            s.flows_to
                .iter()
                .map(move |d| (s.name.as_str(), d.as_str()))
        })
        .collect();
    let summary = format!(
        "{} · {} · {} · {}",
        j.classification.as_str(),
        crate::vocab::count(j.sources.len(), "source"),
        crate::vocab::count(j.destinations.len(), "destination"),
        crate::vocab::count(j.model_endpoints.len(), "model endpoint"),
    );
    // The local→cloud flip must be READABLE, not a JSON-only fact
    // (gauntlet 08-01, Aïcha: the --plain line was byte-identical for
    // mock and mistral while the machine lane knew locus, retention
    // and training). One dim row per distinct CLOUD provider, its
    // sourced facts beside it — an unknown stays the word `unknown`.
    let mut cloud_rows: Vec<String> = Vec::new();
    for e in &j.model_endpoints {
        if e.locus != nika_check::EndpointLocus::Cloud {
            continue;
        }
        let retention = e.retention.as_deref().unwrap_or("unknown");
        let trains = e.trains.as_deref().unwrap_or("unknown");
        let row = format!(
            "cloud endpoint {} · task data leaves this machine · retention {retention} · training {trains}",
            e.provider
        );
        if !cloud_rows.contains(&row) {
            cloud_rows.push(row);
        }
    }
    if flows.is_empty() {
        let _ = writeln!(
            out,
            " {} {} {}",
            mark(t, true),
            t.paint(Role::Strong, "JOURNEY"),
            t.paint(
                Role::Dim,
                &format!("{summary} · no secret reaches an external destination")
            )
        );
        for row in &cloud_rows {
            let _ = writeln!(
                out,
                " {} {} {}",
                mark(t, true),
                t.paint(Role::Strong, "JOURNEY"),
                t.paint(Role::Dim, row)
            );
        }
        return;
    }
    // A flow exists: the headline takes the warn posture (the audit
    // completed, the voyage carries a receipt obligation), and every
    // (secret, destination) pair gets its own named row.
    let _ = writeln!(
        out,
        " {} {} {}",
        t.paint(Role::Warn, if t.ascii { "!" } else { "⚠" }),
        t.paint(Role::Strong, "JOURNEY"),
        t.paint(Role::Dim, &summary)
    );
    for (name, dest) in flows {
        let _ = writeln!(
            out,
            " {} {} secret `{name}` flows to {dest} · review consent before the run",
            t.paint(Role::Warn, if t.ascii { "!" } else { "⚠" }),
            t.paint(Role::Strong, "JOURNEY"),
        );
    }
    for row in &cloud_rows {
        let _ = writeln!(
            out,
            " {} {} {}",
            mark(t, true),
            t.paint(Role::Strong, "JOURNEY"),
            t.paint(Role::Dim, row)
        );
    }
}
