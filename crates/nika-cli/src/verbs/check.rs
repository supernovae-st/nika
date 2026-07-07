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
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::VarDecl;

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, load_checked, load_checked_with_source};

/// The `nika check <file>` verb.
#[must_use]
pub fn run(path: &str, json: bool, theme: Theme) -> VerbOutput {
    let (source, wf, report) = match load_checked_with_source(path) {
        Ok(triple) => triple,
        Err(out) => return out,
    };

    if json {
        return match serde_json::to_value(&report) {
            Ok(mut payload) => {
                let clean = report.is_clean();
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("clean".to_owned(), serde_json::Value::Bool(clean));
                    obj.insert("pricing".to_owned(), pricing_section(&report));
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

    let text = render(&report, &wf, &source, path, theme);
    if report.is_clean() {
        VerbOutput::ok(text)
    } else {
        VerbOutput::file(text)
    }
}

/// The rates the preflight shows BEFORE the first run: each model the
/// requirements collected (#213), priced from the vendored catalog.
/// UNKNOWN is null, never 0.00 — a missing price must look missing.
/// Rates only (USD per 1M tokens): token counts are unknowable
/// statically; the estimate with honest bounds is the next arc.
fn pricing_section(report: &nika_schema::check::CheckReport) -> serde_json::Value {
    let models: Vec<serde_json::Value> = report
        .requirements
        .models
        .iter()
        .map(|m| {
            let priced = nika_catalog::find_pricing_for(&m.model);
            serde_json::json!({
                "model": m.model,
                "input_per_million": priced.map(|p| p.input_per_million),
                "output_per_million": priced.map(|p| p.output_per_million),
            })
        })
        .collect();
    serde_json::json!({ "models": models })
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
fn render(report: &CheckReport, wf: &RawWorkflow, source: &str, path: &str, t: Theme) -> String {
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
        // The rustc `--explain` move: every code links its own page.
        let _ = writeln!(out, "   {}", t.paint(Role::Dim, &c.docs_url));
        // …and span-carrying findings frame the offending source line
        // (rustc-grade caret · the CONFORM row above stays grep-stable).
        if let Some(span) = c.span {
            let frame = crate::display::snippet::paint_span(source, path, span, t);
            let _ = writeln!(out, "{frame}");
        }
    }

    plan(&mut out, report, wf, t);
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
        unknown_tool_rows(report),
    );
    section_list(
        &mut out,
        t,
        "ARGS",
        "every invoke arg key is declared + every required arg is present",
        arg_rows(report),
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
    hints_and_verdict(&mut out, report, wf, t);
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
            format!(
                "`{}` (task `{}`) has no `{}` arg{}",
                u.tool,
                u.task,
                u.arg,
                fix_clause(u.suggestion.as_deref())
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
    let hint_word = if hints == 1 { "hint" } else { "hints" };
    let mark = if t.ascii { "ok" } else { "✔" };
    t.paint(
        Role::Good,
        &format!(
            "{mark} audited · {tasks} task(s) · {} wave(s) · permits {permits} · est {floor}${:.4} · {hints} {hint_word}",
            report.waves.len(),
            report.cost.min_path_total_usd,
        ),
    )
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
        " {} {}     {} wave(s) · {tasks} task(s) · max parallelism {max_par}{width_note}",
        mark(t, true),
        t.paint(Role::Strong, "PLAN"),
        report.waves.len(),
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
        RawAction::Invoke(a) => ("invoke", Some(a.tool.value.clone())),
        // #[non_exhaustive] — a future verb must not break this build.
        _ => ("task", None),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pricing_section_rates_known_null_unknown() {
        let wf = parse_wf(
            "nika: v1\nworkflow: priced\nmodel: anthropic/claude-opus-4-5\ntasks:\n  - id: think\n    infer:\n      prompt: hi\n  - id: odd\n    infer:\n      model: custom/never-heard-of-it\n      prompt: hi\n",
        );
        let report = nika_schema::check(&wf);
        let section = pricing_section(&report);
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

    /// A `required: true` input with no `default:` is what the operator MUST
    /// pass — `check` should NAME it, so a bare `run` does not surprise them
    /// with NIKA-VAR-001.
    #[test]
    fn required_input_without_default_is_listed() {
        let wf = parse_wf(
            "nika: v1\nworkflow: needs-input\nmodel: mock/echo\nvars:\n  text:\n    type: string\n    required: true\ntasks:\n  - id: a\n    infer: { prompt: \"${{ vars.text }}\" }\n",
        );
        assert_eq!(required_inputs(&wf), vec!["text"]);
    }

    /// Untyped (the value IS the default) · typed-with-default · typed-optional
    /// — none block a bare `run`, so none are listed.
    #[test]
    fn defaulted_or_optional_inputs_are_not_listed() {
        let wf = parse_wf(
            "nika: v1\nworkflow: ok\nmodel: mock/echo\nvars:\n  a: \"has default\"\n  b:\n    type: string\n    default: \"d\"\n  c:\n    type: string\n    required: false\ntasks:\n  - id: t\n    infer: { prompt: \"${{ vars.a }} ${{ vars.b }} ${{ vars.c }}\" }\n",
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
        let dir = std::env::temp_dir().join("nika-cli-killtests");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(&path, yaml).expect("fixture body");
        let theme = Theme::new(false, ascii, false);
        run(path.to_str().expect("utf8 path"), false, theme).text
    }

    /// The COST section names a DISTINCT reason per unbounded task — a deleted
    /// match arm collapses one of these into the bare `unbounded` fallback, so
    /// each exact phrase pins its arm: `NoTokenLimit` · `NoPrice` · `UnknownIterations`.
    #[test]
    fn cost_section_names_each_unbounded_reason() {
        let text = checked_text(
            "cost-reasons.nika.yaml",
            "nika: v1\nworkflow: cost-reasons\nvars:\n  items: { type: array, required: true }\ntasks:\n  - id: a\n    infer: { prompt: \"hi\", model: \"anthropic/claude-opus-4-20250514\" }\n  - id: b\n    infer: { prompt: \"hi\", model: \"ollama/llama3.1\", max_tokens: 50 }\n  - id: c\n    for_each: \"${{ vars.items }}\"\n    infer: { prompt: \"x\", model: \"anthropic/claude-opus-4-20250514\", max_tokens: 10 }\n",
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
            "nika: v1\nworkflow: clean-one\ntasks:\n  - id: a\n    exec: { command: \"echo hi\" }\n",
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
        let yaml = "nika: v1\nworkflow: card\nmodel: mock/echo\ntasks:\n  - id: a\n    exec: { command: \"echo hi\" }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"echo bye\" }\n";
        let text = checked_text("audited-card.nika.yaml", yaml, false);
        assert!(
            text.contains(
                "✔ audited · 2 task(s) · 2 wave(s) · permits none · est ≥$0.0000 · 1 hint"
            ),
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-5\ntasks:\n  - id: think\n    infer: { prompt: hi }\n  - id: after\n    depends_on: [think]\n    exec:\n      command: [\"echo\", \"x\"]\n",
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
            "nika: v1\nworkflow: bad-ref\ntasks:\n  - id: a\n    exec: { command: \"echo ${{ vars.nope }}\" }\n",
            true,
        );
        assert!(
            text.contains("(skipped — no valid DAG order while conformance fails)"),
            "{text}"
        );
    }
}
