// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NEXT/verdict footer of `nika check` — provenance-aware repair guidance.

use std::fmt::Write as _;

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;

use super::{
    RepairTarget, Role, Theme, audited_line, findings_line, render_report_hints, required_inputs,
};

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
    (verdict, layers): (bool, &super::VerdictLayers),
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
    let grade = nika_check::risk_grade(report);
    if verdict {
        let _ = writeln!(
            out,
            " {}",
            audited_line(report, wf, distinct_identities.len(), hint_sites, grade, t)
        );
        // Wave 2 · the four questions the green line answered (or not).
        let _ = writeln!(out, " {}", super::layers_line(layers, t));
    } else if unfilled_scaffold(report) {
        // No NEXT after it: the SLOTS rung already ended on the one
        // command, and `nika explain` has nothing to say about a value
        // nobody wrote (the class is report-only — a code would 404).
        scaffold_verdict(out, report, t);
        return;
    } else {
        // The failing verdict carries the SAME boundary summary the
        // green one does (wave 3 · p04): the file with findings is the
        // file an operator most needs summarised.
        let _ = writeln!(
            out,
            " {}",
            findings_line(report, distinct_identities.len(), hint_sites, grade, t)
        );
    }
    render_next(out, report, path, repair_target, hint_sites, t);
}

/// The verdict for a scaffold whose only finding is its own unfilled
/// slots (#1066 constraint 4).
///
/// `✖ findings above` would tell someone who typed `nika new` thirty
/// seconds ago that they broke a file they never wrote. The run still
/// refuses — the exit code is untouched — only the wording matches what
/// actually happened.
fn scaffold_verdict(out: &mut String, report: &CheckReport, t: Theme) {
    let n = report.slot_findings.len();
    let slots = if n == 1 { "slot" } else { "slots" };
    let _ = writeln!(
        out,
        " {} {}",
        t.paint(Role::Warn, if t.ascii { ".." } else { "…" }),
        t.paint(
            Role::Warn,
            &format!("not a workflow yet — {n} {slots} to fill, then it audits")
        )
    );
}

/// Whether the ONLY thing standing between this file and a green audit
/// is its own unfilled slots.
///
/// Deliberately narrow: a scaffold that ALSO escapes its permits or
/// leaks a secret gets the ordinary refusal, because those are real
/// faults and softening them would be the lie in the other direction.
fn unfilled_scaffold(report: &CheckReport) -> bool {
    !report.slot_findings.is_empty() && report.findings.iter().all(|f| f.kind == "slot")
}

/// The ONE next command, when the report left anything to say.
///
/// Advises `--fix` on a workspace file only when `--fix` has something to
/// apply. The trigger used to be `hint_sites > 0` alone, which is a
/// DIFFERENT set: a file whose only findings are hints (`native-first` ·
/// `NIKA-DRIFT-001`) has nothing in the ladder, so `--fix` printed « no
/// machine-applicable repairs » at the top and this line told the reader
/// to run `--fix` at the bottom — of the same output. Obeying it is a
/// fixed point: not one byte changes, and the exit it offers is the
/// command that produced it.
///
/// The other three targets are exempt because they CANNOT loop: their
/// guidance is PROVENANCE (« this cache is read-only · copy it first »),
/// which holds whether or not a rename exists, and obeying it changes the
/// state — there is a new file to check. Suppressing them would hide
/// where a reader may write at all.
fn render_next(
    out: &mut String,
    report: &CheckReport,
    path: &str,
    repair_target: RepairTarget,
    hint_sites: usize,
    t: Theme,
) {
    if hint_sites == 0 {
        return;
    }
    let loops_on_itself = repair_target == RepairTarget::WorkspaceFile;
    let next = if !loops_on_itself || has_machine_applicable_repair(report) {
        format!(
            "{} · see `nika explain` for coded findings",
            next_repair_action(path, repair_target)
        )
    } else {
        "see `nika explain` for coded findings — nothing here is a typed rename, \
         so these are yours to place"
            .to_owned()
    };
    let _ = writeln!(
        out,
        " {} {}     {next}",
        t.paint(Role::Accent, "↳"),
        t.paint(Role::Strong, "NEXT"),
    );
}

/// Whether `check --fix` would change a byte of THIS file.
///
/// The ladder (`nika_cli_host::fix_ladder`) has exactly two sources. The
/// dead-form arms fire on a `SchemaError` — a file that did not parse, and
/// so never reaches this render at all. The typed renames are therefore
/// the whole of it on a file that PARSED, and this asks
/// [`nika_check::typed_renames`] — the SAME derivation the ladder
/// splices from.
///
/// It used to MIRROR that derivation field for field, in its own words.
/// A mirror is only as true as the day it was typed: the offer and the
/// work were free to drift the moment the ladder grew an arm, and the
/// drift would have been silent and in the honest direction — a file
/// told « nothing here is a typed rename » while `--fix` had one to
/// place. One function, two readers, no mirror to keep.
fn has_machine_applicable_repair(report: &CheckReport) -> bool {
    !nika_check::typed_renames(report).is_empty()
}

fn next_repair_action(path: &str, repair_target: RepairTarget) -> String {
    match repair_target {
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
