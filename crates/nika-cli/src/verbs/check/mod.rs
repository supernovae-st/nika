// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check` — the ADR-092 static ladder, rendered (spec §2).
//!
//! The human surface: grep-stable section keywords (CONFORM/PLAN/COST/
//! SECRETS/TYPES/TOOLS/SCHEMA/GATES/PERMITS/HINT) through the ONE colour seam
//! (`display::theme` · semantic-only). The machine surface (`--json`):
//! the full [`CheckReport`] + a `clean` flag, NEVER coloured — the
//! contract bytes are the contract. Check is INFALLIBLE past parse
//! (rustc model): every defect lands in the report, one round-trip.

use std::fmt::Write as _;

use nika_schema::check::{CheckReport, ConformanceViolation, UnboundedReason};
use nika_schema::infer_permits;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::VarDecl;

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, load_checked, load_checked_with_source};

mod models_rung;
use models_rung::{ModelFinding, pricing_section, unresolvable_models};

/// The `nika check <file>` verb. `native_strict` promotes the advisory
/// `native-first` hints to failures (exit 2) — the agent/CI posture:
/// spec-validity is unchanged, but an `exec:` with a probable native
/// path no longer sails through silently.
#[must_use]
pub fn run(
    path: &str,
    json: bool,
    native_strict: bool,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let (source, wf, report) = match load_checked_with_source(path) {
        Ok(triple) => triple,
        // Parse-fatal + `--json` (#331's papercut): the machine mode
        // stays machine-parseable — ONE JSON error object on stdout
        // (parse_fatal + a findings[] row shaped like the report's own),
        // never the plain-text refusal an agent's json parse chokes on.
        Err(out) if json => return parse_fatal_json(&out),
        Err(out) => return out,
    };
    // `--model m` previews the RUN override's static envelope (#342): the
    // report is recomputed with `m` as the effective envelope default —
    // the same substitution the run's budget preflight prices, so what
    // check shows IS what run will refuse or allow.
    let (wf, report) = match model_override {
        Some(m) => {
            let wf = crate::verbs::with_model_override(&wf, m);
            let report = nika_schema::check(&wf);
            (wf, report)
        }
        None => (wf, report),
    };
    let native_hints = report
        .hints
        .iter()
        .filter(|h| h.kind == "native-first")
        .count();
    // The MODELS rung (#320): the ladder validated TOOLS but not MODELS —
    // the exact asymmetry a hallucinating agent hits. A `model:` this
    // binary cannot resolve is a FINDING (exit 2), never a green audit.
    let model_findings = unresolvable_models(&report);
    // SKILLS rung (#473 · MODELS pattern): a bad SKILL.md is a FINDING.
    let skills = super::resolve_workflow_skills(&wf);
    let clean = report.is_clean() && model_findings.is_empty() && skills.findings.is_empty();
    let strict_clean = clean && (!native_strict || native_hints == 0);

    if json {
        return json_verdict(
            &report,
            &model_findings,
            &skills,
            clean,
            strict_clean,
            native_strict,
        );
    }

    let mut text = render(&report, &wf, &source, path, theme, &model_findings, &skills);
    if native_strict && report.is_clean() && native_hints > 0 {
        let hint_word = if native_hints == 1 { "hint" } else { "hints" };
        let _ = writeln!(
            text,
            " {}",
            theme.paint(
                Role::Bad,
                &format!(
                    "✖ native-strict · {native_hints} native-first {hint_word} above — \
                     replace the exec(s) or record them in the exec ledger"
                ),
            )
        );
    }
    if strict_clean {
        VerbOutput::ok(text)
    } else {
        VerbOutput::file(text)
    }
}

/// The parse-fatal machine verdict (#331): a file the parser refuses
/// never reaches the report, but a `--json` consumer still gets JSON —
/// `parse_fatal: true`, `clean: false`, and ONE findings[] row carrying
/// the spec code the plain-text voice prints (`PARSE ✗ [CODE] …`). The
/// exit code (2 file · 3 env) rides unchanged.
fn parse_fatal_json(out: &VerbOutput) -> VerbOutput {
    let text = out.text.trim();
    // The plain voice is `PARSE ✗  [NIKA-…] message` — recover the code;
    // an env-class refusal (unreadable file) has no code and stays codeless.
    let code = text
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(code, _)| code.to_owned());
    let message = text.split_once("] ").map_or(text, |(_, m)| m).to_owned();
    let mut finding = serde_json::json!({
        "kind": "parse",
        "gate": "PARSE",
        "severity": "error",
        "message": message,
    });
    if let Some(c) = &code {
        finding["code"] = serde_json::json!(c);
        finding["docs_url"] =
            serde_json::json!(format!("{}/{c}", nika_schema::check::ERROR_DOCS_BASE));
    }
    let payload = serde_json::json!({
        "report_version": nika_schema::check::REPORT_VERSION,
        "parse_fatal": true,
        "clean": false,
        "findings": [finding],
    });
    VerbOutput {
        text: format!("{payload:#}"),
        code: out.code,
    }
}

/// The `--json` verdict: the full report + the machine keys (`clean` ·
/// `models_resolve` · `model_findings[]` · `skills_resolve` ·
/// `skill_findings[]` · `pricing` · the strict flag) — never coloured,
/// the contract bytes are the contract.
fn json_verdict(
    report: &CheckReport,
    model_findings: &[ModelFinding],
    skills: &nika_schema::ResolvedSkills,
    clean: bool,
    strict_clean: bool,
    native_strict: bool,
) -> VerbOutput {
    let mut payload = match serde_json::to_value(report) {
        Ok(v) => v,
        Err(e) => return VerbOutput::env(format!("cannot serialize report: {e}")),
    };
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("clean".to_owned(), serde_json::Value::Bool(clean));
        obj.insert(
            "models_resolve".to_owned(),
            serde_json::Value::Bool(model_findings.is_empty()),
        );
        if !model_findings.is_empty() {
            obj.insert(
                "model_findings".to_owned(),
                serde_json::Value::Array(
                    model_findings
                        .iter()
                        .map(|f| {
                            serde_json::json!({
                                "model": f.model,
                                "tasks": f.tasks,
                                "why": f.why,
                            })
                        })
                        .collect(),
                ),
            );
        }
        skills.extend_check_json(obj);
        obj.insert(
            "pricing".to_owned(),
            pricing_section(report, model_findings),
        );
        if native_strict {
            obj.insert(
                "native_strict_clean".to_owned(),
                serde_json::Value::Bool(strict_clean),
            );
        }
    }
    let text = format!("{payload:#}");
    if strict_clean {
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
fn render(
    report: &CheckReport,
    wf: &RawWorkflow,
    source: &str,
    path: &str,
    t: Theme,
    model_findings: &[ModelFinding],
    skills: &nika_schema::ResolvedSkills,
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
    // POLICY (spec 10 · W4) · silent when the file binds no law — the
    // rows are the ladder's own findings, code first (one voice with
    // `--json` findings[] and the LSP projection).
    if wf.policy.is_some() {
        section_list(
            &mut out,
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
    hints_and_verdict(&mut out, report, wf, t);
    // The MAP beside the verdict — the same themed wire art `graph
    // --format ascii` speaks, so the audit READS as the DAG it judged
    // (operator ask 2026-07-12: « quand on fait check, voir la dag »).
    // Interactive surface only; conformance failures skip it (no valid
    // wave order exists to draw).
    if t.accents && report.conformance.is_empty() {
        let _ = write!(out, "\n{}", super::graph::ascii_art(wf, report, t));
    }
    out
}

/// Declared `vars:` that the operator MUST pass at run time — `required: true`
/// with no `default:`. The static surface can NAME them (so `check` warns
/// before a bare `run` hits `NIKA-VAR-001`); only the runtime binds them.
fn required_inputs(wf: &RawWorkflow) -> Vec<&str> {
    wf.vars
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
fn hints_and_verdict(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    let mut hint_count = report.hints.len();
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
    // The stranger's first trap (V-arc F1): statically-resolvable
    // `nika:read` paths that do not exist HERE — a hint, never an
    // error (the file may appear at run time). Analysis is
    // nika-schema's; only the filesystem question lives at this edge.
    for (task, path) in nika_schema::check::static_read_paths(wf)
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
    let floor = crate::display::vocab::at_least(t.ascii);
    let mark = if t.ascii { "ok" } else { "✔" };
    t.paint(
        Role::Good,
        &format!(
            "{mark} audited · {} · {} · permits {permits} · est {floor}${:.4} · {}",
            crate::text::count(tasks, "task"),
            crate::text::count(report.waves.len(), "wave"),
            report.cost.min_path_total_usd,
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
            .map(nika_schema::check::CompositionFinding::row)
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

fn permits(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    // With no boundary declared the panel is informational — UNLESS the
    // SSRF-floor parity pass found escapes (the floor is permits-
    // independent): those MUST render, or `✖ findings above` points at
    // nothing (the mute-diagnostic the battery re-run caught).
    if wf.permits.is_none() && report.capability_escapes.is_empty() {
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
        // machine list stamps (floor → SEC-005 · boundary → SEC-004).
        let code = if e.floor {
            "NIKA-SEC-005"
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

/// Several files through the same per-file ladder — the pre-commit / CI
/// shape (`nika check a.nika.yaml b.nika.yaml`). Each file gets the FULL
/// [`run`] report (its header names the file), every file still audits
/// after an earlier failure (no stop-at-first — the hook UX law), and the
/// worst spec-§4 exit survives (3 environment > 2 findings). The machine
/// modes stay one-file-per-call — `report_version: 1` is a per-file
/// contract — so `main` refuses `--json`/`--infer-permits` upstream
/// before this is reached.
#[must_use]
pub fn run_many(
    paths: &[String],
    native_strict: bool,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let mut texts = Vec::with_capacity(paths.len());
    let mut worst = crate::verbs::exit::OK;
    for path in paths {
        let out = run(path, false, native_strict, model_override, theme);
        texts.push(out.text);
        worst = worst.max(out.code);
    }
    VerbOutput {
        text: texts.join("\n"),
        code: worst,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `run_many`: every file audits even after an earlier failure (the
    /// broken file sits in the MIDDLE), each report keeps its own header,
    /// and the worst spec-§4 exit survives.
    #[test]
    fn run_many_audits_every_file_and_keeps_the_worst_exit() {
        let dir = std::env::temp_dir().join(format!("nika-check-many-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let clean = "nika: v1\nworkflow:\n  id: ok\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
        let broken = "nika: v1\nworkflow:\n  id: bad\ntasks:\n  t:\n    infer: { prompt: \"${{ tasks.ghost.output }}\", max_tokens: 10, model: \"mock/echo\" }\n";
        let a = dir.join("many-a.nika.yaml");
        let b = dir.join("many-broken.nika.yaml");
        let c = dir.join("many-c.nika.yaml");
        std::fs::write(&a, clean).expect("fixture a");
        std::fs::write(&b, broken).expect("fixture b");
        std::fs::write(&c, clean).expect("fixture c");

        let paths: Vec<String> = [&a, &b, &c]
            .iter()
            .map(|p| p.to_str().expect("utf8 path").to_owned())
            .collect();
        let out = run_many(&paths, false, None, Theme::new(false, true, false));

        assert_eq!(out.code, 2, "the broken middle file's exit survives");
        // The report header names its file by BASENAME (`nika check · f`).
        for name in [
            "many-a.nika.yaml",
            "many-broken.nika.yaml",
            "many-c.nika.yaml",
        ] {
            assert!(
                out.text.contains(name),
                "every report present (headers name their file): missing {name}\n{}",
                out.text
            );
        }
        let after = out.text.split_once("many-broken.nika.yaml").map(|s| s.1);
        assert!(
            after.is_some_and(|tail| tail.contains("many-c.nika.yaml")),
            "the file AFTER the failure still audited: {}",
            out.text
        );
    }

    /// `run_many` on all-clean files exits OK — the concatenation never
    /// invents a failure.
    #[test]
    fn run_many_is_clean_when_every_file_is() {
        let dir = std::env::temp_dir().join(format!("nika-check-many-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let clean = "nika: v1\nworkflow:\n  id: ok\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
        let a = dir.join("clean-a.nika.yaml");
        let b = dir.join("clean-b.nika.yaml");
        std::fs::write(&a, clean).expect("fixture a");
        std::fs::write(&b, clean).expect("fixture b");
        let paths: Vec<String> = [&a, &b]
            .iter()
            .map(|p| p.to_str().expect("utf8 path").to_owned())
            .collect();
        let out = run_many(&paths, false, None, Theme::new(false, true, false));
        assert_eq!(out.code, 0, "{}", out.text);
    }

    #[test]
    fn missing_read_files_flags_static_literal_and_var_default() {
        let dir = std::env::temp_dir().join(format!("nika-lint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap_or(());
        let present = dir.join("present.txt");
        std::fs::write(&present, "x").expect("fixture");
        let yaml = format!(
            "nika: v1\nworkflow:\n  id: w\nvars:\n  src: \"{missing}\"\ntasks:\n  a:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"${{{{ vars.src }}}}\" }}\n  b:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"{present}\" }}\n  c:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"${{{{ tasks.a.output }}}}\" }}\n",
            missing = dir.join("missing.txt").display(),
            present = present.display(),
        );
        let wf = parse_wf(&yaml);
        let flagged: Vec<(String, String)> = nika_schema::check::static_read_paths(&wf)
            .into_iter()
            .filter(|(_, p)| !std::path::Path::new(p).exists())
            .collect();
        // `a` via var default → flagged · `b` exists → silent ·
        // `c` dynamic (task ref) → the lint never guesses.
        assert_eq!(flagged.len(), 1, "{flagged:?}");
        assert_eq!(flagged[0].0, "a");
        let _ = std::fs::remove_file(&present);
    }

    #[test]
    fn pricing_section_rates_known_null_unknown() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: priced\nmodel: anthropic/claude-opus-4-5\ntasks:\n  think:\n    infer:\n      prompt: hi\n  odd:\n    infer:\n      model: custom/never-heard-of-it\n      prompt: hi\n",
        );
        let report = nika_schema::check(&wf);
        let section = pricing_section(&report, &unresolvable_models(&report));
        let models = section["models"].as_array().expect("array");
        assert_eq!(models.len(), 2, "one row per requirements model");
        let by_model = |name: &str| {
            models
                .iter()
                .find(|m| m["model"] == name)
                .expect("a row per requirements model")
                .clone()
        };
        let priced = by_model("anthropic/claude-opus-4-5");
        assert!((priced["input_per_million"].as_f64().expect("rate") - 5.0).abs() < 1e-9);
        assert!((priced["output_per_million"].as_f64().expect("rate") - 25.0).abs() < 1e-9);
        // UNKNOWN renders null — a missing price must look missing,
        // never $0.00 (the silent-zero anti-pattern).
        let unknown = by_model("custom/never-heard-of-it");
        assert!(unknown["input_per_million"].is_null());
        assert!(unknown["output_per_million"].is_null());
    }

    fn parse_wf(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    /// The mute-diagnostic regression the battery re-run caught: with NO
    /// `permits:` block, a floor escape (SSRF-parity pass · permits-
    /// independent) exited rc=2 while the PERMITS panel printed only the
    /// informational `○ no boundary declared` line — `✖ findings above`
    /// pointed at nothing. The panel must render the escape.
    #[test]
    fn floor_escape_renders_without_a_permits_block() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  probe:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1:8971/x\" } }\n",
        );
        let report = nika_schema::check(&wf);
        assert!(
            !report.capability_escapes.is_empty(),
            "the floor pass fires without permits"
        );
        let theme = Theme::new(false, true, false);
        let mut out = String::new();
        permits(&mut out, &report, &wf, theme);
        assert!(out.contains("SSRF floor"), "escape must render: {out}");
        assert!(
            out.contains("NIKA-SEC-005"),
            "the wire code names it: {out}"
        );
        // A2 (agent battery 2026-07-11): the code LEADS the row in
        // bracket position — `[NIKA-SEC-005 · net]` — so the PERMITS
        // panel is explainable like every CONFORM row (`nika explain`).
        assert!(
            out.contains("[NIKA-SEC-005 · net]"),
            "the code leads the row: {out}"
        );
        assert!(
            !out.contains("no boundary declared"),
            "the informational line must yield to the finding: {out}"
        );
        // …and the informational line still renders when there is nothing
        // to say (the common clean case is unchanged).
        let clean = parse_wf(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  probe:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n",
        );
        let clean_report = nika_schema::check(&clean);
        let mut clean_out = String::new();
        permits(&mut clean_out, &clean_report, &clean, theme);
        assert!(clean_out.contains("no boundary declared"), "{clean_out}");
    }

    /// The #395 admitting direction, through the CLI render: the battery
    /// local-watch repro (`permits.net.http: ["127.0.0.1"]` + a literal
    /// fetch to it) is GREEN — no NIKA-SEC-005, no dead-grant flag — and
    /// the panel TEACHES the clearing with the informational line.
    #[test]
    fn permitted_loopback_literal_renders_green_with_the_teaching_line() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: local-watch\npermits:\n  net: { http: [\"127.0.0.1\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1:8971/price.json\" } }\n",
        );
        let report = nika_schema::check(&wf);
        assert!(
            report.capability_escapes.is_empty(),
            "the exact literal declassifies: {:?}",
            report.capability_escapes
        );
        let theme = Theme::new(false, true, false);
        let mut out = String::new();
        permits(&mut out, &report, &wf, theme);
        assert!(
            out.contains("body fits the declared boundary"),
            "green panel: {out}"
        );
        assert!(
            out.contains("exact loopback literal") && out.contains("`127.0.0.1`"),
            "the teaching line renders: {out}"
        );
        // …and a boundary with no loopback literal renders NO such line.
        let plain = parse_wf(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n",
        );
        let plain_report = nika_schema::check(&plain);
        let mut plain_out = String::new();
        permits(&mut plain_out, &plain_report, &plain, theme);
        assert!(
            !plain_out.contains("exact loopback literal"),
            "no loopback grant → no line: {plain_out}"
        );
    }

    /// A `required: true` input with no `default:` is what the operator MUST
    /// pass — `check` should NAME it, so a bare `run` does not surprise them
    /// with NIKA-VAR-001.
    #[test]
    fn required_input_without_default_is_listed() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: needs-input\nmodel: mock/echo\nvars:\n  text:\n    type: string\n    required: true\ntasks:\n  a:\n    infer: { prompt: \"${{ vars.text }}\" }\n",
        );
        assert_eq!(required_inputs(&wf), vec!["text"]);
    }

    /// Untyped (the value IS the default) · typed-with-default · typed-optional
    /// — none block a bare `run`, so none are listed.
    #[test]
    fn defaulted_or_optional_inputs_are_not_listed() {
        let wf = parse_wf(
            "nika: v1\nworkflow:\n  id: ok\nmodel: mock/echo\nvars:\n  a: \"has default\"\n  b:\n    type: string\n    default: \"d\"\n  c:\n    type: string\n    required: false\ntasks:\n  t:\n    infer: { prompt: \"${{ vars.a }} ${{ vars.b }} ${{ vars.c }}\" }\n",
        );
        assert!(
            required_inputs(&wf).is_empty(),
            "{:?}",
            required_inputs(&wf)
        );
    }

    /// Write a fixture + run the human `check` render over it (ascii/no-colour
    /// so the assertions pin glyphs/text, not ANSI). The render path is what
    /// the operator reads — these tests pin its exact words.
    fn checked_text(name: &str, yaml: &str, ascii: bool) -> String {
        // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
        // matrix · a dev double-run) share the OS tmpdir, and a fixed
        // name let them stomp each other's fixtures mid-read (flaked
        // live 2026-07-10 — the same fixed-temp-name class as the
        // check-expect mktemp collision, #376).
        let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(&path, yaml).expect("fixture body");
        let theme = Theme::new(false, ascii, false);
        run(path.to_str().expect("utf8 path"), false, false, None, theme).text
    }

    /// Same fixture plumbing, full `VerbOutput` (exit-code assertions) —
    /// the `--native-strict` posture tests read `.code`.
    fn checked_output(name: &str, yaml: &str, native_strict: bool) -> VerbOutput {
        // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
        // matrix · a dev double-run) share the OS tmpdir, and a fixed
        // name let them stomp each other's fixtures mid-read (flaked
        // live 2026-07-10 — the same fixed-temp-name class as the
        // check-expect mktemp collision, #376).
        let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(&path, yaml).expect("fixture body");
        let theme = Theme::new(false, true, false);
        run(
            path.to_str().expect("utf8 path"),
            false,
            native_strict,
            None,
            theme,
        )
    }

    /// #320 repro 1: a CATALOGED-but-unresolvable provider (`azure/…` —
    /// the vendor listing knows it, the resolver does not) must be a
    /// finding, exit 2 — never a green audit that dies at run.
    #[test]
    fn models_rung_reds_a_cataloged_but_unresolvable_provider() {
        let out = checked_output(
            "models-azure.nika.yaml",
            "nika: v1\nworkflow:\n  id: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"azure/gpt-4o\" }\n",
            false,
        );
        assert_eq!(
            out.code, 2,
            "unresolvable provider is a finding: {}",
            out.text
        );
        assert!(
            out.text.contains("MODELS") && out.text.contains("`azure`"),
            "the rung names the provider: {}",
            out.text
        );
    }

    /// #320 repro 2: a BARE model id (no `<provider>/` prefix) reds the
    /// rung AND must never wear a conjured price in the pricing section.
    #[test]
    fn models_rung_reds_a_bare_model_id_and_never_conjures_a_price() {
        let out = checked_output(
            "models-bare.nika.yaml",
            "nika: v1\nworkflow:\n  id: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"gpt-5-turbo\" }\n",
            false,
        );
        assert_eq!(out.code, 2, "bare id is a finding: {}", out.text);
        assert!(
            out.text.contains("bare model id"),
            "teaches the contract: {}",
            out.text
        );
        // The JSON surface: models_resolve false · clean false · the
        // pricing row is NULL (unpriced beats conjured — the $0.0001
        // fuzzy-match hole from the live evidence).
        // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
        // matrix · a dev double-run) share the OS tmpdir, and a fixed
        // name let them stomp each other's fixtures mid-read (flaked
        // live 2026-07-10 — the same fixed-temp-name class as the
        // check-expect mktemp collision, #376).
        let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
        let path = dir.join("models-bare.nika.yaml");
        let theme = Theme::new(false, true, false);
        let out = run(path.to_str().expect("utf8 path"), true, false, None, theme);
        assert_eq!(out.code, 2);
        let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
        assert_eq!(payload["clean"], false);
        assert_eq!(payload["models_resolve"], false);
        assert_eq!(
            payload["model_findings"][0]["model"], "gpt-5-turbo",
            "{payload:#}"
        );
        let row = &payload["pricing"]["models"][0];
        assert!(
            row["input_per_million"].is_null() && row["output_per_million"].is_null(),
            "an unresolvable model is never priced: {row:#}"
        );
    }

    /// The happy path: every model resolvable → the rung is one green
    /// line and the audit verdict is untouched.
    #[test]
    fn models_rung_is_green_when_every_model_resolves() {
        let out = checked_output(
            "models-green.nika.yaml",
            "nika: v1\nworkflow:\n  id: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
            false,
        );
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(
            out.text.contains("MODELS") && out.text.contains("1 model resolves"),
            "the green rung is visible: {}",
            out.text
        );
    }

    /// `--json --native-strict`: the payload's `native_strict_clean` and
    /// the exit code must agree (the review-swarm untested-branch gap).
    #[test]
    fn native_strict_json_payload_agrees_with_the_exit_code() {
        let helper = "nika: v1\nworkflow:\n  id: helper\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";
        // Per-PROCESS dir: two concurrent `cargo test` invocations (a CI
        // matrix · a dev double-run) share the OS tmpdir, and a fixed
        // name let them stomp each other's fixtures mid-read (flaked
        // live 2026-07-10 — the same fixed-temp-name class as the
        // check-expect mktemp collision, #376).
        let dir = std::env::temp_dir().join(format!("nika-cli-killtests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("native-strict-json.nika.yaml");
        std::fs::write(&path, helper).expect("fixture body");
        let theme = Theme::new(false, true, false);
        let out = run(path.to_str().expect("utf8 path"), true, true, None, theme);
        assert_eq!(
            out.code, 2,
            "strict hint-only workflow exits FILE: {}",
            out.text
        );
        let payload: serde_json::Value = serde_json::from_str(&out.text).expect("json");
        assert_eq!(
            payload["clean"],
            serde_json::json!(true),
            "spec-clean stays true"
        );
        assert_eq!(
            payload["native_strict_clean"],
            serde_json::json!(false),
            "the strict verdict rides the payload: {payload:#}"
        );
    }

    /// `--native-strict` promotes native-first hints to failure: the SAME
    /// spec-valid workflow exits 0 by default and 2 under strict, with the
    /// strict verdict naming the count; a natively-written twin stays exit
    /// 0 under strict.
    #[test]
    fn native_strict_fails_on_native_first_hints_only() {
        let helper = "nika: v1\nworkflow:\n  id: helper\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";
        let default_run = checked_output("native-default.nika.yaml", helper, false);
        assert_eq!(
            default_run.code, 0,
            "advisory by default: {}",
            default_run.text
        );
        assert!(
            default_run.text.contains("[native-first]"),
            "{}",
            default_run.text
        );

        let strict = checked_output("native-strict.nika.yaml", helper, true);
        assert_eq!(
            strict.code, 2,
            "strict promotes to failure: {}",
            strict.text
        );
        assert!(
            strict.text.contains("native-strict · 1 native-first hint"),
            "{}",
            strict.text
        );

        let native_twin = "nika: v1\nworkflow:\n  id: native\ntasks:\n  crawl:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://acme.test\" } }\n";
        let twin = checked_output("native-twin.nika.yaml", native_twin, true);
        assert_eq!(twin.code, 0, "the native twin passes strict: {}", twin.text);
        assert!(!twin.text.contains("native-strict ·"), "{}", twin.text);
    }

    /// The COST section names a DISTINCT reason per unbounded task — a deleted
    /// match arm collapses one of these into the bare `unbounded` fallback, so
    /// each exact phrase pins its arm: `NoTokenLimit` · `NoPrice` · `UnknownIterations`.
    #[test]
    fn cost_section_names_each_unbounded_reason() {
        let text = checked_text(
            "cost-reasons.nika.yaml",
            "nika: v1\nworkflow:\n  id: cost-reasons\nvars:\n  items: { type: array, required: true }\ntasks:\n  a:\n    infer: { prompt: \"hi\", model: \"anthropic/claude-opus-4-20250514\" }\n  b:\n    infer: { prompt: \"hi\", model: \"ollama/llama3.1\", max_tokens: 50 }\n  c:\n    for_each: \"${{ vars.items }}\"\n    infer: { prompt: \"x\", model: \"anthropic/claude-opus-4-20250514\", max_tokens: 10 }\n",
            true,
        );
        assert!(text.contains("no max_tokens declared"), "{text}");
        assert!(
            text.contains("no catalog price (local/unknown model)"),
            "{text}"
        );
        assert!(
            text.contains("for_each over an expression (unknown count)"),
            "{text}"
        );
    }

    /// `mark()` paints the verdict glyph on EVERY clean section — not just the
    /// one literal verdict line. A mutated mark (returns `""` / `"xyzzy"`)
    /// strips the section glyphs (count drops) or injects a placeholder.
    #[test]
    fn clean_report_marks_every_section() {
        let text = checked_text(
            "clean-one.nika.yaml",
            "nika: v1\nworkflow:\n  id: clean-one\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
            false,
        );
        let ticks = text.matches('✔').count();
        assert!(
            ticks >= 5,
            "every clean section carries ✔ (got {ticks}): {text}"
        );
        assert!(
            !text.contains("xyzzy"),
            "mark never emits a placeholder: {text}"
        );
    }

    /// The clean verdict is the audited CARD line: tasks · waves ·
    /// permits state · the cost floor · the hint count — with full
    /// ASCII parity (`ok audited` · `>=`).
    #[test]
    fn clean_verdict_is_the_audited_card_line() {
        let yaml = "nika: v1\nworkflow:\n  id: card\nmodel: mock/echo\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n  b:\n    after:\n      a: succeeded\n    exec: { command: [\"echo\", \"bye\"] }\n";
        let text = checked_text("audited-card.nika.yaml", yaml, false);
        assert!(
            text.contains("✔ audited · 2 tasks · 2 waves · permits none · est ≥$0.0000 · 1 hint"),
            "the audited card line: {text}"
        );
        let ascii = checked_text("audited-card-ascii.nika.yaml", yaml, true);
        assert!(
            ascii.contains("ok audited") && ascii.contains("est >=$0.0000"),
            "ascii parity (ok · >=): {ascii}"
        );
        assert!(
            !ascii.contains('≥'),
            "no unicode leaks into --ascii: {ascii}"
        );
        // Hint pluralization: 1 hint here (the permits advisory).
        assert!(
            text.contains("1 hint") && !text.contains("1 hints"),
            "{text}"
        );
    }

    /// When conformance FAILS there is no valid DAG, so PLAN announces the skip
    /// (gated on `!conformance.is_empty()`) — a deleted `!` would suppress the
    /// line and leave the operator wondering where the plan went.
    #[test]
    fn plan_prints_wave_membership_with_verbs_and_targets() {
        let text = checked_text(
            "plan-membership.nika.yaml",
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-5\ntasks:\n  think:\n    infer: { prompt: hi }\n  after:\n    after:\n      think: succeeded\n    exec:\n      command: [\"echo\", \"x\"]\n",
            true,
        );
        assert!(text.contains("wave 1"), "membership renders: {text}");
        assert!(
            text.contains("think (infer · anthropic/claude-sonnet-5)"),
            "the envelope model resolves into the plan line: {text}"
        );
        assert!(
            text.contains("after (exec · echo)"),
            "argv[0] names the exec: {text}"
        );
    }

    #[test]
    fn plan_announces_the_skip_when_conformance_fails() {
        let text = checked_text(
            "plan-skip.nika.yaml",
            "nika: v1\nworkflow:\n  id: bad-ref\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ vars.nope }}\"] }\n",
            true,
        );
        assert!(
            text.contains("(skipped — no valid DAG order while conformance fails)"),
            "{text}"
        );
    }
}
