// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check` — the ADR-092 static ladder, rendered (spec §2).
//!
//! The human surface: grep-stable section keywords (CONFORM/PLAN/COST/
//! SECRETS/TYPES/TOOLS/SCHEMA/PERMITS/HINT) through the ONE colour seam
//! (`display::theme` · semantic-only). The machine surface (`--json`):
//! the full [`CheckReport`] + a `clean` flag, NEVER coloured — the
//! contract bytes are the contract. Check is INFALLIBLE past parse
//! (rustc model): every defect lands in the report, one round-trip.

use std::fmt::Write as _;

use nika_schema::check::{CheckReport, UnboundedReason};
use nika_schema::infer_permits;
use nika_schema::raw::RawWorkflow;

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, load_checked};

/// The `nika check <file>` verb.
#[must_use]
pub fn run(path: &str, json: bool, theme: Theme) -> VerbOutput {
    let (wf, report) = match load_checked(path) {
        Ok(pair) => pair,
        Err(out) => return out,
    };

    if json {
        return match serde_json::to_value(&report) {
            Ok(mut payload) => {
                let clean = report.is_clean();
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("clean".to_owned(), serde_json::Value::Bool(clean));
                }
                let text = format!("{payload:#}");
                if clean {
                    VerbOutput::ok(text)
                } else {
                    VerbOutput::file(text)
                }
            }
            Err(e) => VerbOutput::env(format!("cannot serialize report: {e}")),
        };
    }

    let text = render(&report, &wf, path, theme);
    if report.is_clean() {
        VerbOutput::ok(text)
    } else {
        VerbOutput::file(text)
    }
}

/// Section mark: `✔`-class verdict glyphs through the theme seam.
fn mark(theme: Theme, ok: bool) -> String {
    let (uni, asc, role) = if ok {
        ("✔", "ok", Role::Good)
    } else {
        ("✖", "X ", Role::Bad)
    };
    theme.paint(role, if theme.ascii { asc } else { uni })
}

/// Render the human report — every section present, grep-stable keywords.
fn render(report: &CheckReport, wf: &RawWorkflow, path: &str, t: Theme) -> String {
    let mut out = String::new();
    let name = path.rsplit('/').next().unwrap_or(path);
    let _ = writeln!(
        out,
        "{} {}",
        t.paint(Role::Strong, "nika check"),
        t.paint(Role::Dim, &format!("· {name}"))
    );

    for c in &report.conformance {
        let _ = writeln!(
            out,
            " {} {}  [{}] {}",
            mark(t, false),
            t.paint(Role::Strong, "CONFORM"),
            c.code,
            c.message
        );
    }

    plan(&mut out, report, t);
    cost(&mut out, report, t);

    section_list(&mut out, t, "SECRETS", "no information-flow escapes", {
        let mut rows: Vec<String> = report
            .secret_leaks
            .iter()
            .map(|l| format!("leak into {} (task `{}`) — {}", l.sink, l.task, l.trace))
            .collect();
        rows.extend(
            report
                .secret_egresses
                .iter()
                .map(|e| format!("EGRESS via outputs.{} — {}", e.output, e.trace)),
        );
        rows
    });
    section_list(
        &mut out,
        t,
        "TYPES",
        "every deep output reference fits its declared shape",
        report
            .schema_findings
            .iter()
            .map(|f| format!("{} (at `{}`) — {}", f.reference, f.site, f.detail))
            .collect(),
    );
    section_list(
        &mut out,
        t,
        "TOOLS",
        "every nika: tool names a canonical builtin",
        report
            .unknown_tools
            .iter()
            .map(|u| {
                let fix = u
                    .suggestion
                    .as_deref()
                    .map(|s| format!(" · fix: did you mean `{s}`?"))
                    .unwrap_or_default();
                format!(
                    "`{}` (task `{}`) is not a canonical builtin{fix}",
                    u.tool, u.task
                )
            })
            .collect(),
    );
    section_list(
        &mut out,
        t,
        "SCHEMA",
        "every authored schema: is satisfiable",
        report
            .schema_lints
            .iter()
            .map(|l| format!("task `{}` at {} — {}", l.task, l.path, l.detail))
            .collect(),
    );
    permits(&mut out, report, wf, t);
    hints_and_verdict(&mut out, report, t);
    out
}

/// Advisory hints + the one-line verdict (the report's last words).
fn hints_and_verdict(out: &mut String, report: &CheckReport, t: Theme) {
    for h in &report.hints {
        let _ = writeln!(
            out,
            " {} {}     [{}] {}",
            t.paint(Role::Accent, "↳"),
            t.paint(Role::Strong, "HINT"),
            h.kind,
            h.advice
        );
    }
    if report.is_clean() {
        let _ = writeln!(
            out,
            " {}",
            t.paint(
                Role::Good,
                "✔ clean — audited before a single token was spent"
            )
        );
    } else {
        let _ = writeln!(out, " {}", t.paint(Role::Bad, "✖ findings above"));
    }
}

/// A finding section: one OK line when empty, one row per finding else.
fn section_list(out: &mut String, t: Theme, label: &str, ok_msg: &str, rows: Vec<String>) {
    // Pad BEFORE painting — ANSI escapes break `{:<8}` width arithmetic
    // (the format pads bytes, not display columns).
    let padded = format!("{label:<8}");
    if rows.is_empty() {
        let _ = writeln!(
            out,
            " {} {} {}",
            mark(t, true),
            t.paint(Role::Strong, &padded),
            t.paint(Role::Dim, ok_msg)
        );
        return;
    }
    for row in rows {
        let _ = writeln!(
            out,
            " {} {} {row}",
            mark(t, false),
            t.paint(Role::Strong, &padded)
        );
    }
}

fn plan(out: &mut String, report: &CheckReport, t: Theme) {
    if report.waves.is_empty() {
        if !report.conformance.is_empty() {
            let _ = writeln!(
                out,
                " {}     {}",
                t.paint(Role::Strong, "PLAN"),
                t.paint(
                    Role::Dim,
                    "(skipped — no valid DAG order while conformance fails)"
                )
            );
        }
        return;
    }
    let tasks: usize = report.waves.iter().map(Vec::len).sum();
    let max_par = report.waves.iter().map(Vec::len).max().unwrap_or(1);
    let _ = writeln!(
        out,
        " {} {}     {} wave(s) · {tasks} task(s) · max parallelism {max_par}",
        mark(t, true),
        t.paint(Role::Strong, "PLAN"),
        report.waves.len(),
    );
}

fn cost(out: &mut String, report: &CheckReport, t: Theme) {
    if report.cost.tasks.is_empty() {
        let _ = writeln!(
            out,
            " {} {}     {}",
            mark(t, true),
            t.paint(Role::Strong, "COST"),
            t.paint(Role::Dim, "no inference tasks · $0.00")
        );
        return;
    }
    // Unbounded cost is a WARNING posture (is_clean ignores it): the
    // report stays honest about the floor without failing the file.
    let (cost_mark, bound) = if report.cost.has_unbounded {
        (
            t.paint(Role::Warn, if t.ascii { "! " } else { "⚠ " }),
            t.paint(Role::Warn, "FLOOR (unbounded tasks present)"),
        )
    } else {
        (mark(t, true), "worst-case ceiling".to_owned())
    };
    let _ = writeln!(
        out,
        " {cost_mark} {}     {} {bound}",
        t.paint(Role::Strong, "COST"),
        t.paint(
            Role::Strong,
            &format!(
                "${:.4} – ${:.4}",
                report.cost.min_path_total_usd, report.cost.bounded_total_usd
            )
        ),
    );
    for c in &report.cost.tasks {
        let model = c.model.as_deref().unwrap_or("?");
        match (&c.usd, &c.unbounded_reason) {
            (Some(usd), _) => {
                let _ = writeln!(
                    out,
                    "   {}  {}  ≤{} tk  ${usd:.4}",
                    c.task,
                    t.paint(Role::Dim, model),
                    c.max_tokens.unwrap_or(0),
                );
            }
            (None, reason) => {
                let why = match reason {
                    Some(UnboundedReason::NoTokenLimit) => "no max_tokens declared",
                    Some(UnboundedReason::NoPrice) => "no catalog price (local/unknown model)",
                    Some(UnboundedReason::UnknownIterations) => {
                        "for_each over an expression (unknown count)"
                    }
                    _ => "unbounded",
                };
                let _ = writeln!(
                    out,
                    "   {}  {}  {} {}",
                    c.task,
                    t.paint(Role::Dim, model),
                    t.paint(Role::Warn, "UNBOUNDED"),
                    t.paint(Role::Dim, &format!("— {why}")),
                );
            }
        }
    }
}

fn permits(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if wf.permits.is_none() {
        let inferred = infer_permits(wf);
        let _ = writeln!(
            out,
            " {} {}  {}",
            t.paint(Role::Dim, "○"),
            t.paint(Role::Strong, "PERMITS"),
            t.paint(
                Role::Dim,
                "no boundary declared (engine floor only) · `--infer-permits` writes one"
            )
        );
        let _ = inferred; // the inferred boundary ships behind the flag
        return;
    }
    if report.capability_escapes.is_empty() {
        let _ = writeln!(
            out,
            " {} {}  {}",
            mark(t, true),
            t.paint(Role::Strong, "PERMITS"),
            t.paint(Role::Dim, "body fits the declared boundary")
        );
        return;
    }
    for e in &report.capability_escapes {
        let fix = e
            .fix
            .as_deref()
            .map(|f| format!(" · fix: {f}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            " {} {}  [{}] task `{}` · {}{fix}",
            mark(t, false),
            t.paint(Role::Strong, "PERMITS"),
            e.category,
            e.task,
            e.detail,
        );
    }
}

/// `nika check --infer-permits` — write the boundary FOR the operator.
#[must_use]
pub fn run_infer_permits(path: &str, json: bool) -> VerbOutput {
    let (wf, _report) = match load_checked(path) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    let inferred = infer_permits(&wf);
    if json {
        let payload = serde_json::json!({
            "permits_yaml": inferred.to_yaml(),
            "notes": inferred.notes,
        });
        return VerbOutput::ok(format!("{payload:#}"));
    }
    let mut text = inferred.to_yaml();
    if !inferred.notes.is_empty() {
        text.push_str("\n# review — effects too dynamic to pin statically:\n");
        for note in &inferred.notes {
            let _ = writeln!(text, "#   · {note}");
        }
    }
    VerbOutput::ok(text)
}
