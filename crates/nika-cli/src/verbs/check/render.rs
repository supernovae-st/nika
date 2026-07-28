// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The human render surface of `nika check` — the themed report sections
//! (conformance · plan · models · skills · cost · secrets · types ·
//! tools · args · composition · schema · gates · permits · policy ·
//! trifecta · run · hints) and their row builders, split out of `mod.rs`
//! under the ADR-023 1,500-LOC file ceiling (NEP-0002's TRIFECTA rung
//! pushed the verb module over). The machine (`--json`) surface stays in
//! `mod.rs`; both speak the one findings contract.

use std::fmt::Write as _;

use nika_check::{CheckReport, ConformanceViolation, UnboundedReason};
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::VarDecl;

use super::models_rung::ModelFinding;
use crate::display::theme::{Role, Theme};

/// Section mark: `✔`-class verdict glyphs through the theme seam.
pub(super) fn mark(theme: Theme, ok: bool) -> String {
    let (uni, asc, role) = if ok {
        ("✔", "ok", Role::Good)
    } else {
        ("✖", "X ", Role::Bad)
    };
    theme.paint(role, if theme.ascii { asc } else { uni })
}

/// One CONFORM finding: the ✖ row, the offline+online fix pointer (the
/// rustc `--explain` move — the same affordance `run` failures print,
/// #145 P2 · one teaching voice on both surfaces), and the source frame
/// when the finding carries a span (rustc-grade caret · the CONFORM row
/// above stays grep-stable).
fn conformance_row(out: &mut String, c: &ConformanceViolation, source: &str, path: &str, t: Theme) {
    let _ = writeln!(
        out,
        " {} {}  [{}] {}",
        mark(t, false),
        t.paint(Role::Strong, "CONFORM"),
        c.code,
        c.message
    );
    let _ = writeln!(
        out,
        "   {}",
        t.paint(
            Role::Dim,
            &format!("fix: nika explain {} · {}", c.code, c.docs_url)
        )
    );
    if let Some(span) = c.span {
        let frame = crate::display::snippet::paint_span(source, path, span, t);
        let _ = writeln!(out, "{frame}");
    }
}

/// Render the human report — every section present, grep-stable keywords.
pub(super) fn render(
    report: &CheckReport,
    wf: &RawWorkflow,
    source: &str,
    path: &str,
    t: Theme,
    model_findings: &[ModelFinding],
    skills: &nika_schema::ResolvedSkills,
    drift_hints: &[String],
) -> String {
    let mut out = String::new();
    let name = path.rsplit('/').next().unwrap_or(path);
    let _ = writeln!(
        out,
        "{} {}",
        t.paint(Role::Strong, "nika check"),
        t.paint(Role::Dim, &format!("· {name}"))
    );

    for c in &report.conformance {
        conformance_row(&mut out, c, source, path, t);
    }

    plan(&mut out, report, wf, t);
    models(&mut out, report, model_findings, t);
    // SKILLS (#473) · silent when nothing is referenced (rows self-teach).
    if let Some((ok_msg, rows)) = skills.rung() {
        section_list(&mut out, t, "SKILLS", &ok_msg, rows);
    }
    cost(&mut out, report, t);

    section_list(&mut out, t, "SECRETS", "no information-flow escapes", {
        let mut rows: Vec<String> = report
            .secret_leaks
            .iter()
            .map(|l| {
                // The per-sink sanction ON THE SECRET — the human voice
                // matches the `--json` findings[] (one contract · use-case
                // battery 2026-07-11 · T2).
                format!(
                    "leak into {} (task `{}`) — {} · fix: sanction it — \
                     `egress: [{{ to: \"{}\" }}]` on `secrets.{}`",
                    l.sink, l.task, l.trace, l.sink_id, l.secret
                )
            })
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
        // Narrowed on purpose. The scan is sound (schema_typing.rs: an
        // opaque shape resolves to "unknown — no finding", never a
        // guess), but the old line — "every deep output reference fits
        // its declared shape" — read as universal. It is not: a builtin
        // has no way to declare an output shape, so `output.total_usd`
        // on a `nika:inspect` task is UNCHECKED, not checked-and-fine.
        // A green that means less than it says spends trust and returns
        // nothing; this one now claims exactly what it covers.
        "deep references fit the shapes tasks declare · builtin output has none",
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
        unknown_tool_rows(report),
    );
    section_list(
        &mut out,
        t,
        "ARGS",
        "every invoke arg key is declared + every required arg is present",
        arg_rows(report),
    );
    composition_rung(&mut out, report, wf, t);
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
    section_list(
        &mut out,
        t,
        "GATES",
        "every task is statically reachable · status literals in vocabulary",
        gate_rows(report),
    );
    permits(&mut out, report, wf, t);
    policy_rung(&mut out, report, wf, t);
    trifecta_rung(&mut out, report, wf, t);
    run_rung(&mut out, report, wf, t);
    hints_and_verdict(&mut out, report, wf, t, drift_hints);
    // The MAP beside the verdict — the same themed wire art `graph
    // --format ascii` speaks, so the audit READS as the DAG it judged
    // (operator ask 2026-07-12: « quand on fait check, voir la dag »).
    // Interactive surface only; conformance failures skip it (no valid
    // wave order exists to draw).
    if t.accents && report.conformance.is_empty() {
        let _ = write!(out, "\n{}", crate::verbs::graph::ascii_art(wf, report, t));
    }
    out
}

/// POLICY rung (spec 10 · W4) · silent when the file binds no law — the
/// rows are the ladder's own findings, code first (one voice with
/// `--json` findings[] and the LSP projection).
fn policy_rung(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if wf.policy.is_some() {
        section_list(
            out,
            t,
            "POLICY",
            "every hard policy: rule holds (soft families recorded, not judged)",
            report
                .policy_findings
                .iter()
                .map(|p| format!("[NIKA-POLICY-001] {}", p.detail))
                .collect(),
        );
    }
}

/// TRIFECTA rung (NEP-0002) · only when a boundary is declared — without
/// `permits:` the legs are not decidable as declared and the lane is
/// inert (the default-deny/floor lanes own that world).
fn trifecta_rung(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if wf.permits.is_some() {
        section_list(
            out,
            t,
            "TRIFECTA",
            "no lethal trifecta without a dominating human gate",
            report
                .trifecta_findings
                .iter()
                .map(|f| format!("[NIKA-SEC-009] {}", f.detail))
                .collect(),
        );
    }
}

/// RUN rung (F-P3) · only when the envelope declares `run:` — an absent
/// block is the undeclared status quo and stays SILENT (the existing
/// corpus renders unchanged). The rows are the declaration
/// contradictions, code first (one voice with `--json` findings[]).
fn run_rung(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if wf.run.is_some() {
        section_list(
            out,
            t,
            "RUN",
            "the declared entropy/clock seams hold (F-P3)",
            report
                .run_decl_findings
                .iter()
                .map(|f| {
                    format!(
                        "[{}] task `{}` · {} · fix: {}",
                        f.wire_code(),
                        f.task,
                        f.detail,
                        f.fix
                    )
                })
                .collect(),
        );
    }
}

/// Declared `inputs:` that the operator MUST pass at run time —
/// `required: true` with no `default:`. The static surface can NAME them
/// (so `check` warns before a bare `run` hits `NIKA-VAR-001`); only the
/// runtime binds them.
pub(super) fn required_inputs(wf: &RawWorkflow) -> Vec<&str> {
    wf.inputs
        .iter()
        .filter_map(|(name, decl)| match decl {
            VarDecl::Typed {
                required: true,
                default: None,
                ..
            } => Some(name.value.as_str()),
            _ => None,
        })
        .collect()
}

/// The `· fix: did you mean ___?` clause, or empty when no suggestion.
fn fix_clause(suggestion: Option<&str>) -> String {
    suggestion
        .map(|s| format!(" · fix: did you mean `{s}`?"))
        .unwrap_or_default()
}

/// One row per gate-liveness refusal (DAG-006 statically dead · DAG-007
/// out-of-vocabulary status literal) — code first, one-voice.
fn gate_rows(report: &CheckReport) -> Vec<String> {
    report
        .gate_findings
        .iter()
        .map(|g| {
            let fix = g
                .fix
                .as_deref()
                .map(|f| format!(" · fix: {f}"))
                .unwrap_or_default();
            format!(
                "[{}] task `{}` — {}{fix}",
                g.kind.wire_code(),
                g.task,
                g.detail
            )
        })
        .collect()
}

/// One row per `nika:` tool that names no canonical builtin.
fn unknown_tool_rows(report: &CheckReport) -> Vec<String> {
    report
        .unknown_tools
        .iter()
        .map(|u| {
            format!(
                "`{}` (task `{}`) is not a canonical builtin{}",
                u.tool,
                u.task,
                fix_clause(u.suggestion.as_deref())
            )
        })
        .collect()
}

/// The ARGS section rows — undeclared arg keys (the typo class) THEN
/// missing required args (the « passed check {} then failed at run » class).
fn arg_rows(report: &CheckReport) -> Vec<String> {
    let mut rows: Vec<String> = report
        .unknown_args
        .iter()
        .map(|u| {
            // With a suggestion the fix is the rename; without one (a
            // wrong-name-entirely miss — `extract` for fetch's `mode`),
            // the closed declared set is the teaching.
            let teach = if u.suggestion.is_some() {
                fix_clause(u.suggestion.as_deref())
            } else {
                format!(" — declared: {}", u.declared.join(" · "))
            };
            format!(
                "`{}` (task `{}`) has no `{}` arg{teach}",
                u.tool, u.task, u.arg,
            )
        })
        .collect();
    rows.extend(report.missing_args.iter().map(|m| {
        format!(
            "`{}` (task `{}`) is missing required `{}`",
            m.tool, m.task, m.arg
        )
    }));
    rows
}

/// Advisory hints + the one-line verdict (the report's last words).
fn hints_and_verdict(
    out: &mut String,
    report: &CheckReport,
    wf: &RawWorkflow,
    t: Theme,
    drift_hints: &[String],
) {
    let mut hint_count = report.hints.len() + drift_hints.len();
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
    // NIKA-DRIFT-001 rows — the declared-vs-unused family, computed at
    // this edge (super::drift); the code-first bracket voice matches the
    // PERMITS rows (`[NIKA-SEC-005 · net]`).
    for advice in drift_hints {
        let _ = writeln!(
            out,
            " {} {}     [{} · drift] {}",
            t.paint(Role::Accent, "↳"),
            t.paint(Role::Strong, "HINT"),
            super::drift::DRIFT_CODE,
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
        hint_count += 1;
        let _ = writeln!(
            out,
            " {} {}     [inputs] `{task}` reads `{path}` which does not exist here — create it (or point its var elsewhere) · the run would fail at that wave",
            t.paint(Role::Accent, "↳"),
            t.paint(Role::Strong, "HINT"),
        );
    }
    let inputs = required_inputs(wf);
    if !inputs.is_empty() {
        hint_count += 1;
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
    if report.is_clean() {
        let _ = writeln!(out, " {}", audited_line(report, wf, hint_count, t));
    } else {
        let _ = writeln!(out, " {}", t.paint(Role::Bad, "✖ findings above"));
    }
}

/// The clean verdict as ONE informative card line — what was proven,
/// at a glance: `✔ audited · N tasks · M waves · permits <state> ·
/// est ≥$X · K hints`. The hints themselves stay above; this line
/// counts them so a scroll-past never misses advice silently.
fn audited_line(report: &CheckReport, wf: &RawWorkflow, hints: usize, t: Theme) -> String {
    let tasks: usize = report.waves.iter().map(Vec::len).sum();
    let permits = if wf.permits.is_some() {
        "declared"
    } else {
        "none"
    };
    let mark = if t.ascii { "ok" } else { "✔" };
    // The COST section speaks CEILING throughout — `≤N tk` per task, and
    // the range labelled "worst-case ceiling". This line used to speak
    // FLOOR (`est ≥$X`) over `min_path_total_usd`, and the two
    // contradicted each other in the same output.
    //
    // The floor was never true. `min_path_total_usd` is the cheapest PATH
    // with every task priced at its own token ceiling, so it bounds
    // nothing from below: measured, a run billed $0.000242 under an
    // announced `est ≥$0.0305` — 126× the other way. Users provision
    // against this number before they launch.
    //
    // Bounded: quote the ceiling the section already computed, with `≤`.
    // Unbounded: no ceiling exists, and no floor is computable either
    // (every bounded task is itself priced at its cap), so claim neither
    // — name the uncapped tasks instead.
    let uncapped = report
        .cost
        .tasks
        .iter()
        .filter(|c| c.unbounded_reason.is_some())
        .count();
    let est = if report.cost.has_unbounded {
        format!(
            "est unbounded · {}",
            crate::text::count(uncapped, "uncapped task")
        )
    } else {
        let at_most = crate::display::vocab::at_most(t.ascii);
        format!("est {at_most}${:.4}", report.cost.bounded_total_usd)
    };
    t.paint(
        Role::Good,
        &format!(
            "{mark} audited · {} · {} · permits {permits} · {est} · {}",
            crate::text::count(tasks, "task"),
            crate::text::count(report.waves.len(), "wave"),
            crate::text::count(hints, "hint"),
        ),
    )
}

/// A finding section: one OK line when empty, one row per finding else.
/// COMPOSITION (spec 14) · silent when the workflow calls no child
/// (the SKILLS precedent) — split out of `render` at the 100-line cap.
fn composition_rung(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if report.composition.is_empty() && !wf_calls_workflows(wf) {
        return;
    }
    section_list(
        out,
        t,
        "COMPOSITION",
        "every child call is static, typed, contained and acyclic",
        report
            .composition
            .iter()
            .map(nika_check::CompositionFinding::row)
            .collect(),
    );
}

/// Whether any task (main or `on_finally`) calls a child workflow —
/// the COMPOSITION rung renders only then (silent-when-absent · the
/// SKILLS precedent).
fn wf_calls_workflows(wf: &RawWorkflow) -> bool {
    wf.tasks.iter().any(|task| {
        let is_call = |a: &RawAction| {
            matches!(a, RawAction::Invoke(inv)
                if matches!(inv.target, nika_schema::raw::RawInvokeTarget::Workflow(_)))
        };
        is_call(&task.value.action)
            || task
                .value
                .on_finally
                .iter()
                .any(|m| is_call(&m.value.action))
    })
}

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

fn plan(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
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
    // The wave peak is what the wave scheduler executes; the DAG's exact
    // width (Dilworth · report.analysis) can exceed it — say so when it
    // does, with the witness teaching WHICH tasks could run together.
    let width_note = report
        .analysis
        .as_ref()
        .filter(|a| a.width > max_par)
        .map(|a| {
            let mut sample = a.width_witness.clone();
            sample.truncate(4);
            let suffix = if a.width_witness.len() > 4 {
                " · …"
            } else {
                ""
            };
            format!(
                " · width {} (the DAG permits {} concurrent · e.g. {}{suffix})",
                a.width,
                a.width,
                sample.join(" · "),
            )
        })
        .unwrap_or_default();
    let _ = writeln!(
        out,
        " {} {}     {} · {} · max parallelism {max_par}{width_note}",
        mark(t, true),
        t.paint(Role::Strong, "PLAN"),
        crate::text::count(report.waves.len(), "wave"),
        crate::text::count(tasks, "task"),
    );
    // The membership — WHAT dispatches WHEN (the dry-run answer: check
    // is the dry-run; this line is what `run` will do, wave by wave).
    // Compact workflows only: past 12 tasks the summary line carries it.
    if tasks <= 12 {
        for (i, wave) in report.waves.iter().enumerate() {
            let members: Vec<String> = wave
                .iter()
                .filter_map(|&ix| wf.tasks.get(ix))
                .map(|task| {
                    let (verb, target) = verb_of(&task.value.action, wf);
                    match target {
                        Some(target) => format!("{} ({verb} · {target})", task.value.id.value),
                        None => format!("{} ({verb})", task.value.id.value),
                    }
                })
                .collect();
            let _ = writeln!(
                out,
                "      {} {}",
                t.paint(Role::Dim, &format!("wave {}", i + 1)),
                members.join(" · ")
            );
        }
    }
}

/// A task's verb + dispatch target (model · tool · `argv[0]`) for the plan.
fn verb_of<'w>(action: &'w RawAction, wf: &'w RawWorkflow) -> (&'static str, Option<String>) {
    match action {
        RawAction::Infer(a) => (
            "infer",
            a.model
                .as_ref()
                .map(|m| m.value.clone())
                .or_else(|| wf.model.as_ref().map(|m| m.value.clone())),
        ),
        RawAction::Agent(a) => (
            "agent",
            a.model
                .as_ref()
                .map(|m| m.value.clone())
                .or_else(|| wf.model.as_ref().map(|m| m.value.clone())),
        ),
        RawAction::Exec(a) => (
            "exec",
            match &a.command {
                nika_schema::raw::RawCommand::Shell(_) => Some("sh -c".to_owned()),
                nika_schema::raw::RawCommand::Argv(argv) => argv.first().map(|w| w.value.clone()),
                // #[non_exhaustive] — a future command form names itself.
                _ => None,
            },
        ),
        RawAction::Invoke(a) => match &a.target {
            nika_schema::raw::RawInvokeTarget::Tool(t) => ("invoke", Some(t.value.clone())),
            nika_schema::raw::RawInvokeTarget::Workflow(w) => {
                ("invoke", Some(format!("workflow:{}", w.value)))
            }
        },
        // #[non_exhaustive] — a future verb must not break this build.
        _ => ("task", None),
    }
}

/// The MODELS rung (#320): every `model:` must resolve in THIS binary —
/// green means runnable, never merely cataloged. Renders between PLAN
/// and COST (resolvability before price).
fn models(out: &mut String, report: &CheckReport, findings: &[ModelFinding], t: Theme) {
    if report.requirements.models.is_empty() {
        return; // no inference tasks — the ladder says so at COST already
    }
    if findings.is_empty() {
        let n = report.requirements.models.len();
        let noun = if n == 1 {
            "model resolves"
        } else {
            "models resolve"
        };
        let _ = writeln!(
            out,
            " {} {}   {}",
            mark(t, true),
            t.paint(Role::Strong, "MODELS"),
            t.paint(Role::Dim, &format!("{n} {noun} in this binary"))
        );
        return;
    }
    for f in findings {
        let _ = writeln!(
            out,
            " {} {}   `{}` (task{} {}) — {}",
            mark(t, false),
            t.paint(Role::Strong, "MODELS"),
            f.model,
            if f.tasks.len() == 1 { "" } else { "s" },
            f.tasks.join(", "),
            f.why
        );
    }
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

pub(super) fn permits(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    // F-O8 « absent = zero authority »: with no boundary declared AND
    // nothing escaping (pure compute), the panel states the zero —
    // informational, and `permits: {}` is taught as the legal explicit
    // form. Any escape (absent → NIKA-AUTH-006 · declared → NIKA-SEC-004
    // · floor → NIKA-SEC-005) MUST render, or `✖ findings above` points
    // at nothing (the mute-diagnostic the battery re-run caught). The
    // NEP-0004 taint findings (interpolated bound → NIKA-AUTH-007 ·
    // untrusted argument escape → NIKA-AUTH-008) ride the SAME panel.
    if wf.permits.is_none() && report.capability_escapes.is_empty() {
        let _ = writeln!(
            out,
            " {} {}  {}",
            t.paint(Role::Dim, "○"),
            t.paint(Role::Strong, "PERMITS"),
            t.paint(
                Role::Dim,
                "zero authority (no `permits:` declared · F-O8) · pure compute · `permits: {{}}` states it"
            )
        );
        return;
    }
    if report.capability_escapes.is_empty()
        && report.permit_taints.is_empty()
        && report.sink_findings.is_empty()
    {
        let _ = writeln!(
            out,
            " {} {}  {}",
            mark(t, true),
            t.paint(Role::Strong, "PERMITS"),
            t.paint(Role::Dim, "body fits the declared boundary")
        );
        loopback_declassification_lines(out, wf, t);
        return;
    }
    for e in &report.capability_escapes {
        let fix = e
            .fix
            .as_deref()
            .map(|f| format!(" · fix: {f}"))
            .unwrap_or_default();
        // The wire code leads the row (agent battery A2 · 2026-07-11):
        // CONFORM rows print `[NIKA-…]` and teach `nika explain <CODE>`;
        // the PERMITS rows printed only the category, so the one panel
        // whose findings are security-graded was the one panel a user
        // could not ask the engine to explain. Same code the findings[]
        // machine list stamps (floor → SEC-005 · absent boundary →
        // AUTH-006 · declared boundary → SEC-004).
        let code = if e.floor {
            "NIKA-SEC-005"
        } else if e.undeclared {
            "NIKA-AUTH-006"
        } else {
            "NIKA-SEC-004"
        };
        let _ = writeln!(
            out,
            " {} {}  [{code} · {}] task `{}` · {}{fix}",
            mark(t, false),
            t.paint(Role::Strong, "PERMITS"),
            e.category,
            e.task,
            e.detail,
        );
    }
    for sink in &report.sink_findings {
        // NEP-0006 · the finding's own code (the same one findings[] and
        // extra_conformance_codes read).
        let code = sink.wire_code();
        let _ = writeln!(
            out,
            " {} {}  [{code}] task `{}` · {} · fix: {}",
            mark(t, false),
            t.paint(Role::Strong, "PERMITS"),
            sink.task,
            sink.detail,
            sink.fix,
        );
    }
    for taint in &report.permit_taints {
        // NEP-0004 — the finding's own kind IS the code (one arm, the
        // same one findings[] and extra_conformance_codes read).
        let code = taint.wire_code();
        let fix = taint
            .fix
            .as_deref()
            .map(|f| format!(" · fix: {f}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            " {} {}  [{code}] task `{}` · {}{fix}",
            mark(t, false),
            t.paint(Role::Strong, "PERMITS"),
            taint.task,
            taint.detail,
        );
    }
}

/// The loopback-declassification statement (#395), one dim line per
/// exact loopback literal in the declared `net.http`: a green panel must
/// TEACH that the always-on SSRF floor is cleared for that host by the
/// author's explicit permit — silence would hide a security-relevant
/// clearing. Informational only (never a finding — the clearing is the
/// declared intent working as designed); the JSON twin rides
/// `report.permits.notes`.
fn loopback_declassification_lines(out: &mut String, wf: &RawWorkflow, t: Theme) {
    let Some(permits) = wf.permits.as_ref() else {
        return;
    };
    let Some(net) = permits.value.net.as_ref() else {
        return;
    };
    for entry in &net.http {
        if nika_types::net::is_exact_loopback_literal(entry) {
            let _ = writeln!(
                out,
                "   {}",
                t.paint(
                    Role::Dim,
                    &format!(
                        "`{entry}` — exact loopback literal: the explicit permit \
                         clears the always-on SSRF floor (NIKA-SEC-005) for that host"
                    )
                ),
            );
        }
    }
}
