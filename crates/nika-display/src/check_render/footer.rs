// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NEXT/verdict footer of `nika check` — provenance-aware repair guidance.

use std::fmt::Write as _;

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;

use super::{RepairTarget, Role, Theme, audited_line, mark, render_report_hints, required_inputs};

/// Advisory hints + the one-line verdict (the report's last words).
/// `verdict` is the caller's ONE verdict (see [`super::render`]) — this
/// footer shows it, it never re-decides it.
pub(super) fn hints_and_verdict(
    out: &mut String,
    report: &CheckReport,
    wf: &RawWorkflow,
    path: &str,
    repair_target: RepairTarget,
    t: Theme,
    drift_hints: &[String],
    verdict: bool,
) {
    let mut distinct_identities = render_report_hints(out, report, t);
    let mut hint_sites = report.hints.len() + drift_hints.len();
    if !drift_hints.is_empty() {
        distinct_identities.insert(nika_dap::drift::DRIFT_CODE);
    }
    // NIKA-DRIFT-001 rows — the declared-vs-unused family, computed at
    // this edge (super::drift); the code-first bracket voice matches the
    // PERMITS rows (`[NIKA-SEC-005 · net]`).
    for advice in drift_hints {
        let _ = writeln!(
            out,
            " {} {}     [{} · drift] {}",
            t.paint(Role::Accent, "↳"),
            t.paint(Role::Strong, "HINT"),
            nika_dap::drift::DRIFT_CODE,
            advice
        );
    }
    // The stranger's first trap (V-arc F1): statically-resolvable
    // `nika:read` paths that do not exist HERE — a hint, never an
    // error (the file may appear at run time). Analysis is
    // nika-schema's; only the filesystem question lives at this edge.
    for (task, path) in nika_check::static_read_paths(wf)
        .into_iter()
        .filter(|(_, p)| !std::path::Path::new(p).exists())
    {
        hint_sites += 1;
        distinct_identities.insert("inputs");
        let _ = writeln!(
            out,
            " {} {}     [inputs] `{task}` reads `{path}` which does not exist here — create it (or point its var elsewhere) · the run would fail at that wave",
            t.paint(Role::Accent, "↳"),
            t.paint(Role::Strong, "HINT"),
        );
    }
    let inputs = required_inputs(wf);
    if !inputs.is_empty() {
        hint_sites += 1;
        distinct_identities.insert("inputs");
        let pass = inputs
            .iter()
            .map(|n| format!("--var {n}=…"))
            .collect::<Vec<_>>()
            .join(" ");
        let advice = format!("required input(s) with no default · pass at run time: {pass}");
        let _ = writeln!(
            out,
            " {} {}     [inputs] {advice}",
            t.paint(Role::Accent, "↳"),
            t.paint(Role::Strong, "HINT"),
        );
    }
    if verdict {
        let grade = nika_check::risk_grade(report);
        let _ = writeln!(
            out,
            " {}",
            audited_line(report, wf, distinct_identities.len(), hint_sites, grade, t)
        );
    } else {
        // Through `mark()`, not a hardcoded glyph: this line shipped a
        // literal `✖` and was the one verdict in the report that leaked
        // unicode under `--ascii` — the flag exists for terminals that
        // cannot render it, and the failing verdict was exactly the row
        // they could not read.
        let _ = writeln!(
            out,
            " {} {}",
            mark(t, false),
            t.paint(Role::Bad, "findings above")
        );
    }
    if hint_sites > 0 {
        let next = match repair_target {
            RepairTarget::Stdin => {
                "save stdin to a file, then run `nika check --fix <file>` to apply safe repairs and re-check"
                    .to_owned()
            }
            RepairTarget::RegistryArtifact => {
                "copy the registry artifact into your workspace, then run `nika check --fix <copy>` — the digest-pinned cache stays read-only"
                    .to_owned()
            }
            RepairTarget::NonRegularSource => {
                "save or copy this non-regular source into a regular workspace file, then run `nika check --fix <copy>`"
                    .to_owned()
            }
            RepairTarget::WorkspaceFile => {
                let end_of_options = if path.starts_with('-') { " --" } else { "" };
                format!(
                    "run `nika check --fix{end_of_options} {}` to apply safe repairs, then re-check",
                    shell_quote(path)
                )
            }
        };
        let _ = writeln!(
            out,
            " {} {}     {next} · see `nika explain` for coded findings",
            t.paint(Role::Accent, "↳"),
            t.paint(Role::Strong, "NEXT"),
        );
    }
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
