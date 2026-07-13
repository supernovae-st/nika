// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Map a parse error OR a [`CheckReport`] to LSP [`Diagnostic`]s.
//!
//! ONE source of truth: the LSP runs the SAME `nika_schema::parse` +
//! `nika_schema::check` ladder the `nika check` CLI runs, so the codes and
//! messages are byte-identical to the command line — no second checker, no
//! drift (spec §6 « One source of truth for diagnostics »).
//!
//! Severity map ·
//! - **ERROR** — every defect class: conformance violations, schema-type
//!   findings, gate findings, unknown tools/args, missing args, schema
//!   lints, secret leaks/egresses, capability escapes (these are the
//!   classes [`CheckReport::is_clean`] gates on).
//! - **HINT** — the advisory `hints` (« this could be better », never a
//!   failure).
//!
//! Span handling · a finding that carries a [`ByteSpan`] (conformance,
//! gate) renders the exact source range; a task-keyed finding (tools, args,
//! secrets, escapes, schema lints, hints — no span of its own) anchors on
//! the offending task's `id` source range, resolved from the parsed
//! workflow. The two spanless dataflow classes (schema-type findings,
//! secret egresses) carry no task name and render at the document origin.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Range};
use nika_schema::check::ByteSpan;
use nika_schema::raw::RawWorkflow;
use nika_schema::{CheckReport, SchemaError};

use super::position::LineIndex;

/// The diagnostic source label every published diagnostic carries.
const SOURCE: &str = "nika";

/// Map a [`SchemaError`] (the `parse` failure) to a single ERROR
/// diagnostic. The error's span (when present) gives the range; absent, the
/// whole document's first position is used.
#[must_use]
pub fn from_parse_error(index: &LineIndex, err: &SchemaError) -> Diagnostic {
    let range = err
        .span()
        .map(|s| byte_span_range(index, s.start.0, s.end.0))
        .unwrap_or_default();
    let code = err.spec_code().to_string();
    error_diag(range, Some(code), err.to_string())
}

/// Map a [`CheckReport`] to the full set of diagnostics. Every defect
/// class becomes an ERROR; `hints` become HINTs. Task-keyed findings (which
/// carry no span of their own) anchor on the offending task's `id` source
/// range — built from `wf` — instead of the document origin.
#[must_use]
pub fn from_report(index: &LineIndex, report: &CheckReport, wf: &RawWorkflow) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let task_spans = task_id_spans(wf);
    push_spanned_findings(&mut diags, index, report);
    push_tool_findings(&mut diags, index, report, &task_spans);
    push_security_findings(&mut diags, index, report, &task_spans);
    diags
}

/// Findings that carry their OWN source span (conformance + gate-reachability).
fn push_spanned_findings(diags: &mut Vec<Diagnostic>, index: &LineIndex, report: &CheckReport) {
    // Conformance violations — carry a code + span.
    for v in &report.conformance {
        let range = span_or_origin(index, v.span);
        diags.push(error_diag(range, Some(v.code.clone()), v.message.clone()));
    }
    // Gate-reachability findings — carry a span; the code is the kind.
    for g in &report.gate_findings {
        let range = span_or_origin(index, g.span);
        let mut msg = g.detail.clone();
        if let Some(fix) = &g.fix {
            msg.push_str("\nfix: ");
            msg.push_str(fix);
        }
        diags.push(error_diag(range, Some(gate_code(g.kind)), msg));
    }
}

/// Tool / arg / schema findings — task-keyed (anchored on the task `id`),
/// except the dataflow schema-type findings which carry no task name.
fn push_tool_findings(
    diags: &mut Vec<Diagnostic>,
    index: &LineIndex,
    report: &CheckReport,
    task_spans: &BTreeMap<&str, (u32, u32)>,
) {
    for t in &report.unknown_tools {
        let mut msg = format!("unknown tool `{}` in task `{}`", t.tool, t.task);
        if let Some(s) = &t.suggestion {
            push_did_you_mean(&mut msg, s);
        }
        diags.push(error_diag(
            task_range(index, task_spans, &t.task),
            None,
            msg,
        ));
    }
    for a in &report.unknown_args {
        let mut msg = format!(
            "tool `{}` (task `{}`) has no arg `{}`",
            a.tool, a.task, a.arg
        );
        if let Some(s) = &a.suggestion {
            push_did_you_mean(&mut msg, s);
        } else {
            // No honest guess (wrong-name-entirely) — the closed declared
            // set is the teaching, same voice as check/findings.
            let _ = write!(msg, " — declared: {}", a.declared.join(" · "));
        }
        diags.push(error_diag(
            task_range(index, task_spans, &a.task),
            None,
            msg,
        ));
    }
    for m in &report.missing_args {
        let msg = format!(
            "tool `{}` (task `{}`) is missing required arg `{}`",
            m.tool, m.task, m.arg
        );
        diags.push(error_diag(
            task_range(index, task_spans, &m.task),
            None,
            msg,
        ));
    }
    for l in &report.schema_lints {
        let msg = format!(
            "schema defect at `{}` (task `{}`): {}",
            l.path, l.task, l.detail
        );
        diags.push(error_diag(
            task_range(index, task_spans, &l.task),
            None,
            msg,
        ));
    }
    // Dataflow schema-type findings — no task field, anchored at the origin.
    for s in &report.schema_findings {
        let msg = format!(
            "`{}` at `{}` is invalid against `{}`: {}",
            s.reference, s.site, s.target, s.detail
        );
        diags.push(error_diag(Range::default(), None, msg));
    }
}

/// Secret-flow + capability findings, plus advisory hints (HINT severity).
fn push_security_findings(
    diags: &mut Vec<Diagnostic>,
    index: &LineIndex,
    report: &CheckReport,
    task_spans: &BTreeMap<&str, (u32, u32)>,
) {
    for leak in &report.secret_leaks {
        let msg = format!(
            "secret `{}` can leak into `{}` (task `{}`): {}",
            leak.secret, leak.sink, leak.task, leak.trace
        );
        diags.push(error_diag(
            task_range(index, task_spans, &leak.task),
            None,
            msg,
        ));
    }
    // Secret egresses — no task field, anchored at the origin.
    for eg in &report.secret_egresses {
        let msg = format!(
            "secret `{}` leaves the run via output `{}`: {}",
            eg.secret, eg.output, eg.trace
        );
        diags.push(error_diag(Range::default(), None, msg));
    }
    for esc in &report.capability_escapes {
        let mut msg = format!(
            "task `{}` escapes the `{}` permit: {}",
            esc.task, esc.category, esc.detail
        );
        if let Some(fix) = &esc.fix {
            msg.push_str("\nfix: ");
            msg.push_str(fix);
        }
        diags.push(error_diag(
            task_range(index, task_spans, &esc.task),
            None,
            msg,
        ));
    }
    // Advisory hints — never a failure (anchored on the task).
    for hint in &report.hints {
        let msg = format!("[{}] {}: {}", hint.kind, hint.task, hint.advice);
        diags.push(hint_diag(task_range(index, task_spans, &hint.task), msg));
    }
}

/// Map every task id → its `id` source range `(start, end)` byte offsets.
/// The parser emits a point span for the id scalar, widened here to the id's
/// byte length so the anchor highlights the whole id token.
fn task_id_spans(wf: &RawWorkflow) -> BTreeMap<&str, (u32, u32)> {
    wf.tasks
        .iter()
        .map(|t| {
            let start = t.value.id.span.start.0;
            let len = u32::try_from(t.value.id.value.len()).unwrap_or(0);
            (
                t.value.id.value.as_str(),
                (start, start.saturating_add(len)),
            )
        })
        .collect()
}

/// The source range of a task's `id` by name; the document origin when the
/// name is unknown (a finding naming a task the document does not define).
fn task_range(index: &LineIndex, spans: &BTreeMap<&str, (u32, u32)>, task: &str) -> Range {
    spans
        .get(task)
        .map_or_else(Range::default, |&(s, e)| byte_span_range(index, s, e))
}

/// An ERROR-severity diagnostic.
fn error_diag(range: Range, code: Option<String>, message: String) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: code.map(NumberOrString::String),
        source: Some(SOURCE.to_owned()),
        message,
        ..Diagnostic::default()
    }
}

/// A HINT-severity diagnostic (advisory).
fn hint_diag(range: Range, message: String) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::HINT),
        code: None,
        source: Some(SOURCE.to_owned()),
        message,
        ..Diagnostic::default()
    }
}

/// Append a "did you mean" repair line naming `suggestion` to a diagnostic
/// message (the machine-applicable fix the check report carries).
fn push_did_you_mean(msg: &mut String, suggestion: &str) {
    msg.push_str("\ndid you mean `");
    msg.push_str(suggestion);
    msg.push_str("`?");
}

/// Resolve an optional [`ByteSpan`] to a range — the span when present,
/// else the document origin (zero-width at 0,0).
fn span_or_origin(index: &LineIndex, span: Option<ByteSpan>) -> Range {
    span.map_or_else(Range::default, |s| byte_span_range(index, s.start, s.end))
}

/// Convert a `[start, end)` byte span to an LSP range.
fn byte_span_range(index: &LineIndex, start: u32, end: u32) -> Range {
    Range::new(index.position(start as usize), index.position(end as usize))
}

/// The diagnostic code for a gate finding kind.
fn gate_code(kind: nika_schema::GateFindingKind) -> String {
    match kind {
        nika_schema::GateFindingKind::DeadTask => "NIKA-DAG-DEAD".to_owned(),
        nika_schema::GateFindingKind::BadStatusLiteral => "NIKA-DAG-STATUS".to_owned(),
        // The enum is #[non_exhaustive] — a future kind degrades to a
        // generic gate code rather than a build break.
        _ => "NIKA-DAG-GATE".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, check, parse};

    fn index_of(yaml: &str) -> LineIndex {
        LineIndex::new(yaml)
    }

    fn diags_of(yaml: &str) -> Vec<Diagnostic> {
        let index = index_of(yaml);
        match parse(yaml, FileId::new(0), ParseMode::Strict) {
            Ok(wf) => from_report(&index, &check(&wf), &wf),
            Err(err) => vec![from_parse_error(&index, &err)],
        }
    }

    #[test]
    fn clean_workflow_has_no_error_diagnostics() {
        let yaml = "nika: v1\nworkflow: clean\ntasks:\n  - id: a\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let diags = diags_of(yaml);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .collect();
        assert!(errors.is_empty(), "clean workflow → no errors: {errors:?}");
    }

    #[test]
    fn unknown_dep_yields_dag002_error_with_span() {
        // depends_on a ghost task — NIKA-DAG-002, carries a span.
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    depends_on: [ghost]\n    exec: { command: [\"x\"] }\n";
        let diags = diags_of(yaml);
        let dag = diags
            .iter()
            .find(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "NIKA-DAG-002"))
            .expect("NIKA-DAG-002 diagnostic present");
        assert_eq!(dag.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(dag.source.as_deref(), Some("nika"));
        // span points at the `ghost` token, not the origin
        let ghost_byte = yaml.find("ghost").expect("token");
        let expected = index_of(yaml).position(ghost_byte);
        assert_eq!(dag.range.start, expected, "range anchors to the token");
    }

    #[test]
    fn unknown_tool_yields_error_with_did_you_mean() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n";
        let diags = diags_of(yaml);
        let tool = diags
            .iter()
            .find(|d| d.message.contains("unknown tool `nika:raed`"))
            .expect("unknown tool diagnostic");
        assert_eq!(tool.severity, Some(DiagnosticSeverity::ERROR));
        assert!(
            tool.message.contains("did you mean `nika:read`?"),
            "{}",
            tool.message
        );
    }

    #[test]
    fn task_keyed_finding_anchors_on_the_task_id_not_origin() {
        // an unknown tool carries no span of its own — it must anchor on the
        // offending task's `id` source range, NOT the document origin (0,0).
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n";
        let diags = diags_of(yaml);
        let tool = diags
            .iter()
            .find(|d| d.message.contains("unknown tool"))
            .expect("unknown tool diagnostic");
        assert_ne!(tool.range, Range::default(), "anchored, not at the origin");
        let id_byte = yaml.find("id: a").map(|p| p + "id: ".len()).expect("id");
        let expected = index_of(yaml).position(id_byte);
        assert_eq!(tool.range.start, expected, "anchored on the task id `a`");
    }

    #[test]
    fn malformed_yaml_yields_single_parse_error() {
        let yaml = "nika: v1\nworkflow: [unterminated\n";
        let diags = diags_of(yaml);
        assert_eq!(diags.len(), 1, "one parse error: {diags:?}");
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(diags[0].code.is_some(), "parse error carries a spec code");
    }

    #[test]
    fn dead_task_gate_uses_the_nika_dag_dead_code() {
        // A contradictory `when:` gate makes task `b` unreachable. The gate
        // finding renders with the EXACT gate code (not the generic
        // NIKA-DAG-GATE fallback, not an empty/garbage string), ERROR
        // severity, the `nika` source, and a span on the `when:` expression.
        let yaml = "nika: v1\nworkflow: w\nmodel: anthropic/c\ntasks:\n  - id: a\n    exec: { command: [\"true\"] }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' && tasks.a.status == 'failure' }}\n    exec: { command: [\"true\"] }\n";
        let diags = diags_of(yaml);
        let dead = diags
            .iter()
            .find(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "NIKA-DAG-DEAD"))
            .expect("NIKA-DAG-DEAD gate diagnostic present");
        assert_eq!(
            dead.code,
            Some(NumberOrString::String("NIKA-DAG-DEAD".to_owned())),
            "DeadTask maps to its own code, not the generic gate fallback"
        );
        assert_eq!(dead.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(dead.source.as_deref(), Some("nika"));
        assert!(dead.message.contains("can never run"), "{}", dead.message);
        // no diagnostic carries the generic fallback code for this workflow
        assert!(
            !diags.iter().any(
                |d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "NIKA-DAG-GATE")
            ),
            "the DeadTask arm must NOT fall through to NIKA-DAG-GATE"
        );
    }

    #[test]
    fn bad_status_literal_gate_uses_the_nika_dag_status_code() {
        // `!= 'failed'` flags the wrong status literal — a BadStatusLiteral
        // gate finding with its OWN code (deleting the match arm would
        // degrade it to the generic NIKA-DAG-GATE fallback).
        let yaml = "nika: v1\nworkflow: w\nmodel: anthropic/c\ntasks:\n  - id: a\n    exec: { command: [\"true\"] }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status != 'failed' }}\n    exec: { command: [\"true\"] }\n";
        let diags = diags_of(yaml);
        let bad = diags
            .iter()
            .find(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "NIKA-DAG-STATUS"))
            .expect("NIKA-DAG-STATUS gate diagnostic present");
        assert_eq!(bad.severity, Some(DiagnosticSeverity::ERROR));
        assert!(bad.message.contains("not a status"), "{}", bad.message);
        assert!(
            bad.message.contains("fix: did you mean 'failure'?"),
            "the gate fix line is appended: {}",
            bad.message
        );
        assert!(
            !diags.iter().any(
                |d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "NIKA-DAG-GATE")
            ),
            "BadStatusLiteral must NOT fall through to NIKA-DAG-GATE"
        );
    }

    #[test]
    fn hint_diagnostic_carries_exact_fields() {
        // an infer task with no max_tokens triggers the `cost` hint. The
        // HINT diagnostic is a precise object: HINT severity, NO code, the
        // `nika` source, a non-empty message anchored on the task id, and a
        // range over the offending task's id span (NOT the document origin).
        let yaml = "nika: v1\nworkflow: w\nmodel: ollama/llama3\ntasks:\n  - id: a\n    infer: { prompt: \"hi\" }\n";
        let diags = diags_of(yaml);
        let hint = diags
            .iter()
            .find(|d| {
                d.severity == Some(DiagnosticSeverity::HINT) && d.message.starts_with("[cost]")
            })
            .expect("a cost hint is emitted");
        assert_eq!(hint.severity, Some(DiagnosticSeverity::HINT));
        assert_eq!(hint.code, None, "hints carry no NIKA code");
        assert_eq!(
            hint.source.as_deref(),
            Some("nika"),
            "the nika source label"
        );
        assert_eq!(
            hint.message,
            "[cost] a: declare `max_tokens` on `a` — the cost report becomes a hard ceiling instead of UNBOUNDED",
            "the exact advice text"
        );
        // anchored on the task `a` id span (line 4, char 8..9), not (0,0).
        let id_byte = yaml.find("id: a").map(|p| p + "id: ".len()).expect("id");
        let index = index_of(yaml);
        let expected_start = index.position(id_byte);
        assert_eq!(hint.range.start, expected_start, "anchored on task id `a`");
        assert_ne!(hint.range, Range::default(), "not the document origin");
    }
}
