// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workflow the human opened, as the Live host adapter looked at it (the
//! crate-private `session::look`): its faces live in
//! [`nika_tui_view::workspace::inspect`] (ADR-143). The one fold of the check
//! facade's audit into the plain facts those faces read stays here, beside
//! the adapter that takes the look (`read`), so the viewer depends on no
//! check facade.

use nika_cli_host::oracle::Audit;
use nika_display::dag_art::GraphDoc;
use nika_tui_view::workspace::inspect::{Checked, Said};

pub use nika_tui_view::workspace::inspect::Inspected;
#[cfg(test)]
pub(crate) use nika_tui_view::workspace::inspect::title_row;

/// What every audited look says it left out.
pub(crate) const NOT_CAPTURED: &str =
    "not captured (UNKNOWN): child workflows, skills, registry references; judged: this file alone";

/// The look of `path` over `source`, read once with `witness`, and the check
/// facade's answer about those same bytes: its audit ([`checked`]) and the
/// graph of its parse, or the parser's refusal (its code and its words).
pub(crate) fn read(
    path: impl Into<String>,
    witness: String,
    source: String,
    audit: Result<(&Audit, GraphDoc), (String, String)>,
) -> Inspected {
    let checked = audit.map(|(audit, graph)| (checked(audit), graph));
    Inspected::read(path, witness, source, checked)
}

/// What the faces read of one audit: the file's own name, the waves, the
/// layers over the file alone and what was not captured, the findings and
/// hints in the check's order, the identity and the grade.
fn checked(audit: &Audit) -> Checked {
    let (report, verdict) = (&audit.report, &audit.verdict);
    let mut unknown = NOT_CAPTURED.to_owned();
    if !verdict.children.is_empty() {
        unknown = format!("{unknown}; it names {}", verdict.children.join(", "));
    }
    let task = |task: &str| format!("task {task}");
    let mut findings: Vec<Said> = (report.findings.iter())
        .map(|f| {
            Said::new(
                f.code.clone(),
                f.kind,
                f.message.clone(),
                f.task.as_deref().map(task),
            )
        })
        .collect();
    findings.extend(verdict.models.findings.iter().map(|m| {
        let place = (!m.tasks.is_empty()).then(|| task(&m.tasks.join(", ")));
        Said::new(
            m.code.clone(),
            "model",
            format!("{} · {}", m.model, m.why),
            place,
        )
    }));
    let hints = (report.hints.iter())
        .map(|h| {
            let place = (h.task != "-").then(|| task(&h.task));
            Said::new(h.code.map(str::to_owned), h.kind, h.advice.clone(), place)
        })
        .collect();
    Checked::new(
        verdict.layers.clone(),
        report.waves.clone(),
        unknown,
        verdict.grade.as_str(),
    )
    .named(audit.wf.workflow.as_ref().map(|n| n.value.clone()))
    .listing(findings, hints)
    .identified(report.workflow_semantic.clone())
}
