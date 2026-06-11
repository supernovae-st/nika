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

use crate::theme::{Glyph, Theme, VerbKind};

/// Render the full human report. `source` is the workflow YAML text —
/// findings that carry a span render their source excerpt from it.
pub(crate) fn render(
    report: &CheckReport,
    wf: &RawWorkflow,
    source: &str,
    path: &str,
    t: Theme,
) -> String {
    let mut out = String::new();
    let file_label = path.rsplit('/').next().unwrap_or(path);
    banner(&mut out, path, t);
    conformance(&mut out, report, source, file_label, t);
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

/// The horizontal rule — unicode or ASCII, themed like everything else.
fn rule(t: Theme) -> String {
    let bar = if t.unicode_glyphs() { "─" } else { "-" };
    t.dim(&bar.repeat(46))
}

fn banner(out: &mut String, path: &str, t: Theme) {
    let name = path.rsplit('/').next().unwrap_or(path);
    let _ = writeln!(
        out,
        "{} {} {}",
        t.accent(t.glyph(Glyph::Banner)),
        t.bold("nika check"),
        t.dim(&format!("{} {name}", t.middot()))
    );
    out.push_str(&rule(t));
    out.push('\n');
}

fn conformance(out: &mut String, report: &CheckReport, source: &str, file_label: &str, t: Theme) {
    for c in &report.conformance {
        let _ = writeln!(
            out,
            " {} {}  {} {}",
            t.err(t.glyph(Glyph::Err)),
            t.bold("CONFORM"),
            t.dim(&format!("[{}]", c.code)),
            c.message
        );
        // rustc-grade: the offending source line, caret under the token
        if let Some(span) = c.span {
            crate::snippet::render_snippet(out, source, file_label, span, t);
        }
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
                t.dim(&format!(
                    "(skipped {} no valid DAG order while conformance fails)",
                    t.mdash()
                ))
            );
        }
        return;
    }
    let task_count: usize = report.waves.iter().map(Vec::len).sum();
    let max_par = report.waves.iter().map(Vec::len).max().unwrap_or(1);
    let _ = writeln!(
        out,
        " {}     {} wave(s) {mid} {} task(s) {mid} max parallelism {}",
        t.bold("PLAN"),
        report.waves.len(),
        task_count,
        max_par,
        mid = t.middot()
    );
    // self-documenting legend — the colour FAMILY names its governing
    // gate, so the DAG reads at a glance (and survives colour loss: the
    // words carry the meaning too).
    let _ = writeln!(
        out,
        "          {}  {} {} {}  {}  {} {} {}",
        t.dim(&format!(
            "{} will-run {} {} gated  ",
            t.glyph(Glyph::Pending),
            t.middot(),
            t.glyph(Glyph::Gated)
        )),
        t.verb(VerbKind::Infer, "infer"),
        t.verb(VerbKind::Agent, "agent"),
        t.dim("= cost"),
        t.dim(t.middot()),
        t.verb(VerbKind::Exec, "exec"),
        t.verb(VerbKind::Invoke, "invoke"),
        t.dim("= effect")
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
                t.dim(t.glyph(Glyph::Gated))
            } else {
                t.dim(t.glyph(Glyph::Pending))
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
                    notes.push(t.warn(&format!("{}×{attempts}", t.glyph(Glyph::Retry))));
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
                notes.push(t.dim(&format!("{} {}", t.glyph(Glyph::Dep), deps.join(", "))));
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
            t.dim(&format!("no inference tasks {} $0.00", t.middot()))
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
                "${:.4} {} ${:.4}",
                report.cost.min_path_total_usd,
                t.ndash(),
                report.cost.bounded_total_usd
            )),
            t.dim(&format!(
                "(cheapest path: gates closed {} first try)",
                t.middot()
            ))
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
            notes.push(t.warn(&format!("{}×{}", t.glyph(Glyph::Retry), c.attempts)));
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
                    "   {}  {model}  {}{} tk{notes}  {}",
                    c.task,
                    t.leq(),
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
                    t.warn(&format!("{} UNBOUNDED", t.glyph(Glyph::Warn))),
                    t.dim(&format!("{} {why}", t.mdash()))
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
            t.err(t.glyph(Glyph::Err)),
            t.bold("SECRETS"),
            t.bold(l.sink),
            t.dim(&format!("(task `{}`)", l.task)),
            t.dim(&format!("{} {}", t.mdash(), l.trace))
        );
    }
    for e in &report.secret_egresses {
        let _ = writeln!(
            out,
            " {} {}  EGRESS via outputs.{} {} {}",
            t.err(t.glyph(Glyph::Err)),
            t.bold("SECRETS"),
            t.bold(&e.output),
            t.dim(&format!("{} {}", t.mdash(), e.trace)),
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
            " {} {}    {} {} {mdash} {}",
            t.err(t.glyph(Glyph::Err)),
            t.bold("TYPES"),
            t.bold(&f.reference),
            t.dim(&format!("(at `{}`)", f.site)),
            f.detail,
            mdash = t.mdash()
        );
    }
}

fn tools(out: &mut String, report: &CheckReport, t: Theme) {
    for u in &report.unknown_tools {
        let _ = writeln!(
            out,
            " {} {}    `{}` {} is not a canonical builtin",
            t.err(t.glyph(Glyph::Err)),
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
            " {} {}   task `{}` at {} {mdash} {}",
            t.err(t.glyph(Glyph::Err)),
            t.bold("SCHEMA"),
            t.bold(&l.task),
            t.dim(&l.path),
            l.detail,
            mdash = t.mdash()
        );
    }
}

fn permits(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if wf.permits.is_none() {
        let _ = writeln!(
            out,
            " {} {}  {}",
            t.dim(t.glyph(Glyph::Pending)),
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
            " {} {}  {} task `{}` {mid} {}",
            t.err(t.glyph(Glyph::Err)),
            t.bold("PERMITS"),
            t.dim(&format!("[{}]", e.category)),
            t.bold(&e.task),
            e.detail,
            mid = t.middot()
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
            t.accent(t.glyph(Glyph::Hint)),
            t.bold("HINT"),
            t.accent(&format!("[{}]", h.kind)),
            h.advice
        );
    }
}

fn verdict(out: &mut String, report: &CheckReport, t: Theme) {
    out.push_str(&rule(t));
    out.push('\n');
    if report.is_clean() {
        let _ = writeln!(
            out,
            " {}",
            t.verdict_ok(&format!(
                "{} clean {} audited before a single token was spent",
                t.glyph(Glyph::Ok),
                t.mdash()
            ))
        );
    } else {
        let _ = writeln!(
            out,
            " {}",
            t.verdict_err(&format!("{} findings above", t.glyph(Glyph::Err)))
        );
    }
}

fn section_ok(out: &mut String, label: &str, msg: &str, t: Theme) {
    let _ = writeln!(
        out,
        " {} {}{}  {}",
        t.ok(t.glyph(Glyph::Ok)),
        t.bold(label),
        " ".repeat(7_usize.saturating_sub(label.len())),
        t.dim(msg)
    );
}

fn fix_line(out: &mut String, fix: &str, t: Theme) {
    let _ = writeln!(
        out,
        "          {} {}",
        t.accent(t.glyph(Glyph::Fix)),
        t.accent(&format!("fix: {fix}"))
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, check, parse};

    /// A chrome-only workflow (no findings · no hints): every rendered
    /// byte is OURS, so the snapshots pin the frame, not library text.
    const CHROME_ONLY: &str = "nika: v1\nworkflow: pipeline\npermits: { exec: [\"true\"] }\ntasks:\n  - id: first\n    exec: { command: \"true\" }\n  - id: second\n    depends_on: [first]\n    exec: { command: \"true\" }\n";

    fn rendered(t: Theme) -> String {
        let wf = parse(CHROME_ONLY, FileId::new(0), ParseMode::Strict).expect("parse");
        let report = check(&wf);
        render(&report, &wf, CHROME_ONLY, "pipeline.nika.yaml", t)
    }

    #[test]
    fn unicode_frame_is_pinned() {
        // The contract: snapshot tests pin BOTH glyph themes. This is the
        // unicode frame, byte-exact (colour off · pure glyph grammar).
        let expected = concat!(
            "◆ nika check · pipeline.nika.yaml\n",
            "──────────────────────────────────────────────\n",
            " PLAN     2 wave(s) · 2 task(s) · max parallelism 1\n",
            "          ○ will-run · ⊘ gated    infer agent = cost  ·  exec invoke = effect\n",
            "   w0 ○ first  exec  \n",
            "   w1 ○ second exec     ← first\n",
            " COST     no inference tasks · $0.00\n",
            " ✔ SECRETS  no information-flow escapes\n",
            " ✔ TYPES    every deep output reference fits its declared shape\n",
            " ✔ PERMITS  body fits the declared boundary\n",
            "──────────────────────────────────────────────\n",
            " ✔ clean — audited before a single token was spent\n",
        );
        assert_eq!(rendered(Theme::new(false, true)), expected);
    }

    #[test]
    fn ascii_frame_is_pinned_and_pure_ascii() {
        // The ASCII first-class theme, byte-exact — AND provably pure
        // ASCII (this test replaces a shell probe that turned out to be
        // locale-broken: BSD grep under LC_ALL=C missed high bytes).
        let expected = concat!(
            "# nika check - pipeline.nika.yaml\n",
            "----------------------------------------------\n",
            " PLAN     2 wave(s) - 2 task(s) - max parallelism 1\n",
            "          . will-run - - gated    infer agent = cost  -  exec invoke = effect\n",
            "   w0 . first  exec  \n",
            "   w1 . second exec     <- first\n",
            " COST     no inference tasks - $0.00\n",
            " ok SECRETS  no information-flow escapes\n",
            " ok TYPES    every deep output reference fits its declared shape\n",
            " ok PERMITS  body fits the declared boundary\n",
            "----------------------------------------------\n",
            " ok clean -- audited before a single token was spent\n",
        );
        let s = rendered(Theme::new(false, false));
        assert_eq!(s, expected);
        assert!(s.is_ascii(), "ascii theme leaked non-ascii: {s:?}");
    }

    #[test]
    fn verb_gate_colours_appear_when_colour_is_on() {
        // exec tasks paint bold-blue (the PERMITS family) in the lanes.
        let s = rendered(Theme::new(true, true));
        assert!(s.contains("\x1b[1;34mexec"), "exec is bold blue: {s:?}");
        // and the legend carries all four verb roles.
        for sgr in ["\x1b[35minfer", "\x1b[1;35magent", "\x1b[34minvoke"] {
            assert!(s.contains(sgr), "legend misses {sgr:?}");
        }
    }
}
