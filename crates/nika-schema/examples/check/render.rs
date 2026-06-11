// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The human renderer — the DAG as wave lanes + semantic sections.
//!
//! Data-ink discipline (cognitive canon P5): every glyph and colour
//! carries meaning — `○` will-run · `⊘` when:-gated · `✔/✖` section
//! verdicts · `↳` machine-applicable fix · dim = secondary. Section
//! keywords (PLAN/COST/SECRETS/…) stay grep-stable. No spinner: the
//! static check is instant — animation belongs to the run surface.

use nika_schema::CheckReport;
use nika_schema::check::UnboundedReason;
use nika_schema::raw::{RawAction, RawWorkflow};

use std::fmt::Write as _;

use crate::theme::{
    G_BANNER, G_DEP, G_ERR, G_FIX, G_GATED, G_HINT, G_OK, G_PENDING, G_RETRY, G_WARN, Theme,
    VerbKind,
};

/// Render the full human report.
pub(crate) fn render(report: &CheckReport, wf: &RawWorkflow, path: &str, t: Theme) -> String {
    let mut out = String::new();
    banner(&mut out, path, t);
    conformance(&mut out, report, t);
    plan(&mut out, report, wf, t);
    cost(&mut out, report, t);
    secrets(&mut out, report, t);
    types(&mut out, report, t);
    tools(&mut out, report, t);
    schema(&mut out, report, t);
    permits(&mut out, report, wf, t);
    hints(&mut out, report, t);
    verdict(&mut out, report, t);
    out
}

fn banner(out: &mut String, path: &str, t: Theme) {
    let name = path.rsplit('/').next().unwrap_or(path);
    let _ = writeln!(
        out,
        "{} {} {}",
        t.accent(G_BANNER),
        t.bold("nika check"),
        t.dim(&format!("· {name}"))
    );
    out.push_str(&t.dim("──────────────────────────────────────────────"));
    out.push('\n');
}

fn conformance(out: &mut String, report: &CheckReport, t: Theme) {
    for c in &report.conformance {
        let _ = writeln!(
            out,
            " {} {}  {} {}",
            t.err(G_ERR),
            t.bold("CONFORM"),
            t.dim(&format!("[{}]", c.code)),
            c.message
        );
    }
}

/// The plan as a DAG in wave lanes — every task one row, grouped by
/// wave, with its verb, gates, retries and upstream deps annotated.
fn plan(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if report.waves.is_empty() {
        if !report.conformance.is_empty() {
            let _ = writeln!(
                out,
                " {}  {}",
                t.bold("PLAN"),
                t.dim("(skipped — no valid DAG order while conformance fails)")
            );
        }
        return;
    }
    let task_count: usize = report.waves.iter().map(Vec::len).sum();
    let max_par = report.waves.iter().map(Vec::len).max().unwrap_or(1);
    let _ = writeln!(
        out,
        " {}     {} wave(s) · {} task(s) · max parallelism {}",
        t.bold("PLAN"),
        report.waves.len(),
        task_count,
        max_par
    );
    // self-documenting legend — the colour FAMILY names its governing
    // gate, so the DAG reads at a glance (and survives colour loss: the
    // words carry the meaning too).
    let _ = writeln!(
        out,
        "          {}  {} {} {}  ·  {} {} {}",
        t.dim("○ will-run · ⊘ gated  "),
        t.verb(VerbKind::Infer, "infer"),
        t.verb(VerbKind::Agent, "agent"),
        t.dim("→ cost"),
        t.verb(VerbKind::Exec, "exec"),
        t.verb(VerbKind::Invoke, "invoke"),
        t.dim("→ effect")
    );

    let id_width = wf
        .tasks
        .iter()
        .map(|task| task.value.id.value.chars().count())
        .max()
        .unwrap_or(0);

    for (n, wave) in report.waves.iter().enumerate() {
        for &i in wave {
            let task = &wf.tasks[i].value;
            let gated = task.when.is_some();
            let glyph = if gated {
                t.dim(G_GATED)
            } else {
                t.dim(G_PENDING)
            };
            let id = format!("{:width$}", task.id.value, width = id_width);
            let id = if gated { t.dim(&id) } else { id };
            // the verb in its governing-gate colour (magenta=cost ·
            // blue=effect) — pad the raw name, THEN paint (ANSI is
            // zero-width, so the column stays aligned).
            let verb = match verb_kind(&task.action) {
                Some(k) => t.verb(k, &format!("{:6}", k.name())),
                None => t.dim(&format!("{:6}", "?")),
            };

            let mut notes: Vec<String> = Vec::new();
            if let Some(retry) = &task.retry {
                let attempts = retry.value.max_attempts;
                if attempts > 1 {
                    notes.push(t.warn(&format!("{G_RETRY}×{attempts}")));
                }
            }
            if task.for_each.is_some() {
                notes.push(t.dim("for_each"));
            }
            if gated {
                notes.push(t.dim("when:"));
            }
            if !task.depends_on.is_empty() {
                let deps: Vec<&str> = task.depends_on.iter().map(|d| d.value.as_str()).collect();
                notes.push(t.dim(&format!("{G_DEP} {}", deps.join(", "))));
            }
            let notes = if notes.is_empty() {
                String::new()
            } else {
                format!("   {}", notes.join("  "))
            };
            let _ = writeln!(
                out,
                "   {} {} {id} {verb}{notes}",
                t.dim(&format!("w{n}")),
                glyph
            );
        }
    }
}

/// Map a raw action to its presentation verb-kind. `None` for a future
/// `#[non_exhaustive]` verb this example has not learnt yet (renders `?`,
/// never a crash).
fn verb_kind(action: &RawAction) -> Option<VerbKind> {
    match action {
        RawAction::Infer(_) => Some(VerbKind::Infer),
        RawAction::Exec(_) => Some(VerbKind::Exec),
        RawAction::Invoke(_) => Some(VerbKind::Invoke),
        RawAction::Agent(_) => Some(VerbKind::Agent),
        _ => None,
    }
}

fn cost(out: &mut String, report: &CheckReport, t: Theme) {
    if report.cost.tasks.is_empty() {
        let _ = writeln!(
            out,
            " {}     {}",
            t.bold("COST"),
            t.dim("no inference tasks · $0.00")
        );
        return;
    }
    let bound = if report.cost.has_unbounded {
        t.warn("FLOOR (unbounded tasks present)")
    } else {
        "ceiling".to_owned()
    };
    let spread = report.cost.bounded_total_usd - report.cost.min_path_total_usd;
    if spread > f64::EPSILON {
        let _ = writeln!(
            out,
            " {}     {} {bound}  {}",
            t.bold("COST"),
            t.bold(&format!(
                "${:.4} – ${:.4}",
                report.cost.min_path_total_usd, report.cost.bounded_total_usd
            )),
            t.dim("(cheapest path: gates closed · first try)")
        );
    } else {
        let _ = writeln!(
            out,
            " {}     {} worst-case {bound}",
            t.bold("COST"),
            t.bold(&format!("${:.4}", report.cost.bounded_total_usd))
        );
    }
    for c in &report.cost.tasks {
        let model = t.dim(c.model.as_deref().unwrap_or("?"));
        let mut notes: Vec<String> = Vec::new();
        if c.iterations > 1 {
            notes.push(t.dim(&format!("×{}", c.iterations)));
        }
        if c.attempts > 1 {
            notes.push(t.warn(&format!("{G_RETRY}×{}", c.attempts)));
        }
        if c.gated {
            notes.push(t.dim("when:"));
        }
        let notes = if notes.is_empty() {
            String::new()
        } else {
            format!("  {}", notes.join(" "))
        };
        match (c.usd, &c.unbounded_reason) {
            (Some(usd), _) => {
                let _ = writeln!(
                    out,
                    "   {}  {model}  ≤{} tk{notes}  {}",
                    c.task,
                    c.max_tokens.unwrap_or(0),
                    t.bold(&format!("${usd:.4}"))
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
                    "   {}  {model}  {} {}",
                    c.task,
                    t.warn(&format!("{G_WARN} UNBOUNDED")),
                    t.dim(&format!("— {why}"))
                );
            }
        }
    }
}

fn secrets(out: &mut String, report: &CheckReport, t: Theme) {
    if report.secret_leaks.is_empty() && report.secret_egresses.is_empty() {
        section_ok(out, "SECRETS", "no information-flow escapes", t);
        return;
    }
    for l in &report.secret_leaks {
        let _ = writeln!(
            out,
            " {} {}  leak into {} {} {}",
            t.err(G_ERR),
            t.bold("SECRETS"),
            t.bold(l.sink),
            t.dim(&format!("(task `{}`)", l.task)),
            t.dim(&format!("— {}", l.trace))
        );
    }
    for e in &report.secret_egresses {
        let _ = writeln!(
            out,
            " {} {}  EGRESS via outputs.{} {} {}",
            t.err(G_ERR),
            t.bold("SECRETS"),
            t.bold(&e.output),
            t.dim(&format!("— {}", e.trace)),
            t.err("(a secret leaves the run)")
        );
    }
}

fn types(out: &mut String, report: &CheckReport, t: Theme) {
    if report.schema_findings.is_empty() {
        section_ok(
            out,
            "TYPES",
            "every deep output reference fits its declared shape",
            t,
        );
        return;
    }
    for f in &report.schema_findings {
        let _ = writeln!(
            out,
            " {} {}    {} {} — {}",
            t.err(G_ERR),
            t.bold("TYPES"),
            t.bold(&f.reference),
            t.dim(&format!("(at `{}`)", f.site)),
            f.detail
        );
    }
}

fn tools(out: &mut String, report: &CheckReport, t: Theme) {
    for u in &report.unknown_tools {
        let _ = writeln!(
            out,
            " {} {}    `{}` {} is not a canonical builtin",
            t.err(G_ERR),
            t.bold("TOOLS"),
            t.bold(&u.tool),
            t.dim(&format!("(task `{}`)", u.task))
        );
        if let Some(s) = &u.suggestion {
            fix_line(out, &format!("did you mean `{s}`?"), t);
        }
    }
}

fn schema(out: &mut String, report: &CheckReport, t: Theme) {
    for l in &report.schema_lints {
        let _ = writeln!(
            out,
            " {} {}   task `{}` at {} — {}",
            t.err(G_ERR),
            t.bold("SCHEMA"),
            t.bold(&l.task),
            t.dim(&l.path),
            l.detail
        );
    }
}

fn permits(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if wf.permits.is_none() {
        let _ = writeln!(
            out,
            " {} {}  {}",
            t.dim(G_PENDING),
            t.bold("PERMITS"),
            t.dim("no boundary declared (engine floor only)")
        );
        return;
    }
    if report.capability_escapes.is_empty() {
        section_ok(out, "PERMITS", "body fits the declared boundary", t);
        return;
    }
    for e in &report.capability_escapes {
        let _ = writeln!(
            out,
            " {} {}  {} task `{}` · {}",
            t.err(G_ERR),
            t.bold("PERMITS"),
            t.dim(&format!("[{}]", e.category)),
            t.bold(&e.task),
            e.detail
        );
        if let Some(fix) = &e.fix {
            fix_line(out, fix, t);
        }
    }
}

fn hints(out: &mut String, report: &CheckReport, t: Theme) {
    for h in &report.hints {
        let _ = writeln!(
            out,
            " {} {}     {} {}",
            t.accent(G_HINT),
            t.bold("HINT"),
            t.accent(&format!("[{}]", h.kind)),
            h.advice
        );
    }
}

fn verdict(out: &mut String, report: &CheckReport, t: Theme) {
    out.push_str(&t.dim("──────────────────────────────────────────────"));
    out.push('\n');
    if report.is_clean() {
        let _ = writeln!(
            out,
            " {}",
            t.verdict_ok(&format!(
                "{G_OK} clean — audited before a single token was spent"
            ))
        );
    } else {
        let _ = writeln!(
            out,
            " {}",
            t.verdict_err(&format!("{G_ERR} findings above"))
        );
    }
}

fn section_ok(out: &mut String, label: &str, msg: &str, t: Theme) {
    let _ = writeln!(
        out,
        " {} {}{}  {}",
        t.ok(G_OK),
        t.bold(label),
        " ".repeat(7_usize.saturating_sub(label.len())),
        t.dim(msg)
    );
}

fn fix_line(out: &mut String, fix: &str, t: Theme) {
    let _ = writeln!(
        out,
        "          {} {}",
        t.accent(G_FIX),
        t.accent(&format!("fix: {fix}"))
    );
}
