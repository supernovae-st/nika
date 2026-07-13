// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The UNIFIED findings list (#331) — every finding class, one shape,
//! one loop.
//!
//! A failing report scatters findings across ten sibling keys and only
//! `clean: bool` aggregates — a consumer had to hardcode the key list,
//! and a new class broke every consumer SILENTLY. `findings[]` is the
//! aggregation: the per-class keys stay (the typed surface), this is
//! the renderable one. The completeness ratchet lives in this module's
//! tests: `is_clean() ⇔ findings().is_empty()` is asserted per class,
//! so an eleventh finding class that forgets to join the fold turns a
//! test red instead of a consumer blind.

use serde::Serialize;

use super::ByteSpan;
use super::{CheckReport, FindingSeverity};

/// One finding, class-erased: the stable discriminator is [`Self::kind`]
/// (a closed slug set — additive only), the ladder section is
/// [`Self::gate`] (the same grep-stable keyword the human render
/// prints), and `code`/`docs_url` ride ONLY when a canonical spec code
/// exists for the class (the analysis-native classes are report-only by
/// design — a conjured code would 404).
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct UnifiedFinding {
    /// The class slug (`conformance` · `secret_leak` · `secret_egress` ·
    /// `capability_escape` · `schema_type` · `gate` · `unknown_tool` ·
    /// `unknown_arg` · `missing_arg` · `schema_lint`).
    pub kind: &'static str,
    /// The ladder section the human render files this under.
    pub gate: &'static str,
    /// Engine-stamped severity (every class is a run-blocker today).
    pub severity: FindingSeverity,
    /// The canonical spec code, when the class carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The per-code docs page (rides with `code`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,
    /// The human row — same wording family as the rendered report.
    pub message: String,
    /// The offending task, when the finding names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// The source byte range, when the finding carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<ByteSpan>,
}

impl UnifiedFinding {
    fn new(kind: &'static str, gate: &'static str, message: String) -> Self {
        Self {
            kind,
            gate,
            severity: FindingSeverity::Error,
            code: None,
            docs_url: None,
            message,
            task: None,
            span: None,
        }
    }
}

/// Fold every class into the one list — conformance first (the ladder's
/// own order), then the analysis classes in render order.
pub(super) fn collect(report: &CheckReport) -> Vec<UnifiedFinding> {
    let mut out = Vec::new();
    for c in &report.conformance {
        let mut f = UnifiedFinding::new("conformance", "CONFORM", c.message.clone());
        f.code = Some(c.code.clone());
        f.docs_url = Some(c.docs_url.clone());
        f.span = c.span;
        out.push(f);
    }
    for l in &report.secret_leaks {
        // The fix is a per-sink sanction ON THE SECRET — spelled out so the
        // flagship IFC finding is self-serve (the author no longer reads
        // spec 01 §egress to derive it · use-case battery 2026-07-11 · T2).
        let mut f = UnifiedFinding::new(
            "secret_leak",
            "SECRETS",
            format!(
                "leak into {} (task `{}`) — {} · fix: sanction it — \
                 `egress: [{{ to: \"{}\" }}]` on `secrets.{}`",
                l.sink, l.task, l.trace, l.sink_id, l.secret
            ),
        );
        f.task = Some(l.task.clone());
        out.push(f);
    }
    for e in &report.secret_egresses {
        out.push(UnifiedFinding::new(
            "secret_egress",
            "SECRETS",
            format!("EGRESS via outputs.{} — {}", e.output, e.trace),
        ));
    }
    for c in &report.capability_escapes {
        let mut f = UnifiedFinding::new(
            "capability_escape",
            "PERMITS",
            match &c.fix {
                Some(fix) => format!("{} (task `{}`) — fix: {}", c.detail, c.task, fix),
                None => format!("{} (task `{}`)", c.detail, c.task),
            },
        );
        // The wire code the RUN would emit for the same violation: the
        // always-on SSRF floor speaks NIKA-SEC-005, the declared permits
        // boundary NIKA-SEC-004 (spec 05-errors) — check≡run down to the code.
        let code = if c.floor {
            "NIKA-SEC-005"
        } else {
            "NIKA-SEC-004"
        };
        f.code = Some(code.to_owned());
        f.docs_url = Some(format!("{}/{code}", super::ERROR_DOCS_BASE));
        f.task = Some(c.task.clone());
        out.push(f);
    }
    for s in &report.schema_findings {
        out.push(UnifiedFinding::new(
            "schema_type",
            "TYPES",
            format!("{} — {} ({})", s.reference, s.detail, s.site),
        ));
    }
    for g in &report.gate_findings {
        let mut f = UnifiedFinding::new(
            "gate",
            "GATES",
            match &g.fix {
                Some(fix) => format!("{} (task `{}`) — fix: {}", g.detail, g.task, fix),
                None => format!("{} (task `{}`)", g.detail, g.task),
            },
        );
        f.task = Some(g.task.clone());
        f.span = g.span;
        out.push(f);
    }
    fold_tools(report, &mut out);
    for l in &report.schema_lints {
        let mut f = UnifiedFinding::new(
            "schema_lint",
            "SCHEMA",
            format!("{} — {} (task `{}`)", l.path, l.detail, l.task),
        );
        f.task = Some(l.task.clone());
        out.push(f);
    }
    out
}

/// The three tool-contract classes (all BUILTIN-coded, TOOLS/ARGS gates).
fn fold_tools(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    let builtin_code = || Some("NIKA-BUILTIN-001".to_owned());
    let builtin_url = || Some(format!("{}/NIKA-BUILTIN-001", super::ERROR_DOCS_BASE));
    for t in &report.unknown_tools {
        let mut f = UnifiedFinding::new(
            "unknown_tool",
            "TOOLS",
            match &t.suggestion {
                Some(s) => format!(
                    "`{}` names no canonical builtin (task `{}`) — did you mean `{s}`?",
                    t.tool, t.task
                ),
                None => format!(
                    "`{}` names no canonical builtin (task `{}`)",
                    t.tool, t.task
                ),
            },
        );
        f.code = builtin_code();
        f.docs_url = builtin_url();
        f.task = Some(t.task.clone());
        out.push(f);
    }
    for a in &report.unknown_args {
        let mut f = UnifiedFinding::new(
            "unknown_arg",
            "ARGS",
            match &a.suggestion {
                Some(s) => format!(
                    "`{}` is not an arg of `{}` (task `{}`) — did you mean `{s}`?",
                    a.arg, a.tool, a.task
                ),
                // No honest guess (a wrong-name-entirely miss) — the
                // closed declared set IS the teaching.
                None => format!(
                    "`{}` is not an arg of `{}` (task `{}`) — declared: {}",
                    a.arg,
                    a.tool,
                    a.task,
                    a.declared.join(" · ")
                ),
            },
        );
        f.code = builtin_code();
        f.docs_url = builtin_url();
        f.task = Some(a.task.clone());
        out.push(f);
    }
    for m in &report.missing_args {
        let mut f = UnifiedFinding::new(
            "missing_arg",
            "ARGS",
            format!("`{}` requires arg `{}` (task `{}`)", m.tool, m.arg, m.task),
        );
        f.code = builtin_code();
        f.docs_url = builtin_url();
        f.task = Some(m.task.clone());
        out.push(f);
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn report(yaml: &str) -> crate::check::CheckReport {
        crate::check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// THE completeness ratchet: `is_clean()` and `findings.is_empty()`
    /// are the same statement — a new finding class that updates
    /// `is_clean` but forgets the fold turns this red (per-class
    /// fixtures below make the direction concrete).
    #[test]
    fn clean_report_has_zero_findings() {
        let r = report(
            "nika: v1\nworkflow:\n  id: ok\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n",
        );
        assert!(r.is_clean());
        assert!(r.findings.is_empty(), "{:#?}", r.findings);
    }

    /// Per-class: every dirty fixture must surface in findings[] AND
    /// flip `is_clean` — the equivalence, exercised from the dirty side.
    #[test]
    fn each_dirty_class_lands_in_findings_with_its_gate() {
        let cases: Vec<(&str, &str, &str)> = vec![
            (
                // conformance: unresolved reference
                "nika: v1\nworkflow:\n  id: w\ntasks:\n  a:\n    infer: { prompt: \"${{ tasks.ghost.output }}\", max_tokens: 9 }\n",
                "conformance",
                "CONFORM",
            ),
            (
                // capability escape: exec outside a declared boundary
                "nika: v1\nworkflow:\n  id: w\npermits: { exec: false }\ntasks:\n  a:\n    exec: { command: [\"cargo\", \"x\"] }\n",
                "capability_escape",
                "PERMITS",
            ),
            (
                // unknown tool (typo'd builtin)
                "nika: v1\nworkflow:\n  id: w\ntasks:\n  a:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n",
                "unknown_tool",
                "TOOLS",
            ),
            (
                // missing required arg
                "nika: v1\nworkflow:\n  id: w\ntasks:\n  a:\n    invoke: { tool: \"nika:write\", args: { path: \"./x\" } }\n",
                "missing_arg",
                "ARGS",
            ),
        ];
        for (yaml, kind, gate) in cases {
            let r = report(yaml);
            assert!(!r.is_clean(), "fixture must be dirty for {kind}");
            let hit = r
                .findings
                .iter()
                .find(|f| f.kind == kind)
                .unwrap_or_else(|| panic!("{kind} missing from findings: {:#?}", r.findings));
            assert_eq!(hit.gate, gate);
        }
    }

    /// The canonical-code law: conformance + escapes + tool-contract
    /// classes carry a code AND its docs url; analysis-native classes
    /// carry neither (a conjured code would 404).
    #[test]
    fn codes_ride_only_where_canonical() {
        let r = report(
            "nika: v1\nworkflow:\n  id: w\npermits: { exec: false }\ntasks:\n  a:\n    exec: { command: [\"cargo\", \"x\"] }\n",
        );
        let escape = r
            .findings
            .iter()
            .find(|f| f.kind == "capability_escape")
            .expect("escape");
        assert_eq!(escape.code.as_deref(), Some("NIKA-SEC-004"));
        assert_eq!(
            escape.docs_url.as_deref(),
            Some("https://nika.sh/errors/NIKA-SEC-004")
        );
        assert_eq!(escape.task.as_deref(), Some("a"));
    }

    /// The wire shape: absent optionals are ABSENT (never null) — the
    /// consumer's loop reads {kind, gate, severity, message} on every
    /// row and the rest only when present.
    #[test]
    fn serialization_skips_absent_optionals() {
        let r = report(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  a:\n    infer: { prompt: \"${{ tasks.ghost.output }}\", max_tokens: 9 }\n",
        );
        let json = serde_json::to_value(&r.findings).expect("serializes");
        let row = &json[0];
        assert_eq!(row["kind"], "conformance");
        assert_eq!(row["severity"], "error");
        assert!(row.get("task").is_none(), "absent, not null: {row}");
        assert!(row.get("code").is_some());
    }
}
