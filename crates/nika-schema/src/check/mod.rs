// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check` — the static pre-flight (audit-before-it-runs).
//!
//! Because the language is statically analyzable BY CONSTRUCTION (acyclic
//! DAG · bounded `for_each` · non-Turing CEL · declared effects), this
//! module answers « what will this workflow do, cost, and touch? » with
//! **zero API calls and zero tokens spent** — the property no other AI
//! workflow runner gives (spec `07-conformance.md` §`nika check`).
//!
//! It composes [`analyze`](crate::analyze) (Core conformance) with three
//! computed reports over the [`RawWorkflow`] ·
//!
//! - **plan** — the topological wave structure (who runs in parallel)
//! - **cost ceiling** — worst-case OUTPUT spend · `Σ max_tokens × output
//!   price` · input/prompt cost is prompt-dependent (statically unbounded)
//!   so it is excluded · the figure is an output-token ceiling
//! - **secret leaks** — `secrets.X` flowing into an `exec`/tool capture
//! - **capability escapes** — effects outside a declared `permits:` block
//!   (exec program · tool surface · `nika:fetch` host · `nika:read`/`write`
//!   path · for the literal cases · dynamic values stay the runtime check)
//!
//! The spec's fourth `nika check` guarantee — provider parity — is
//! STRUCTURAL, not a separate scan: the strict envelope parser already
//! rejects any non-canonical verb field, so a workflow that parses uses
//! only provider-agnostic fields and runs identically on all providers by
//! construction. `check` is read-only and never executes a verb.

mod analysis;
mod certificate;
mod cost;
mod flow;
mod hints;
mod infer_permits;
#[cfg(test)]
mod metamorphic;
mod permits_fit;
mod reach;
mod schema_lint;
mod schema_typing;
mod secrets;
mod tools;

use crate::analyzer::{self, AnalyzedWorkflow};
use crate::raw::RawWorkflow;

pub use analysis::{DagAnalysis, TaskBlast};
pub use certificate::{Bound, CertTerm, RunCertificate};
pub use cost::{CostCeiling, TaskCost, UnboundedReason};
pub use flow::{FlowFacts, TaintTrace};
pub use hints::Hint;
pub use infer_permits::InferredPermits;
pub use permits_fit::CapabilityEscape;
pub use reach::{GateFinding, GateFindingKind, STATUS_VOCAB};
pub use schema_lint::SchemaLintFinding;
pub use schema_typing::SchemaTypeFinding;
pub use secrets::{SecretEgress, SecretLeak};
pub use tools::UnknownTool;

/// The JSON contract version of [`CheckReport`] — bumped on any
/// breaking field rename/removal so agent loops fail LOUDLY instead of
/// silently misparsing (additive fields do not bump it).
pub const REPORT_VERSION: u32 = 1;

/// A source byte range, report-shaped (serializable — the JSON agent
/// surface + the snippet renderer + the future LSP all read offsets
/// against the source they already hold).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ByteSpan {
    /// Start byte offset (inclusive).
    pub start: u32,
    /// End byte offset (exclusive).
    pub end: u32,
}

impl ByteSpan {
    /// Create a byte span (invariant #19 — `#[non_exhaustive]` structs
    /// ship a constructor).
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// One Core-conformance violation, in report shape (the canonical spec
/// code + the rendered message, which carries the location and any
/// did-you-mean repair).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ConformanceViolation {
    /// The canonical spec code (`NIKA-VAR-001` · `NIKA-DAG-002` · …).
    pub code: String,
    /// The rendered diagnostic (location + did-you-mean included).
    pub message: String,
    /// The source byte range, when the error carries one — renderers
    /// show the offending source line with a caret (rustc-grade
    /// diagnostics for workflow YAML). Additive: `report_version`
    /// stays 1.
    pub span: Option<ByteSpan>,
}

/// The static pre-flight report — everything `nika check` learns without
/// running anything. Serializes to JSON (the agent-facing surface: a
/// generator loop reads the findings + their machine-applicable
/// suggestions, repairs, and re-checks until clean).
///
/// The report is MAXIMAL per run (the rustc model): Core-conformance
/// violations land in [`CheckReport::conformance`] and every analysis
/// that does not need the topological order (cost · tools · schema lint
/// · escapes · typing · hints) still runs — one round-trip tells the
/// agent everything. Only the plan and the IFC secret analysis require
/// a valid DAG and stay empty when conformance fails.
#[derive(Debug, Clone, serde::Serialize)]
#[non_exhaustive]
pub struct CheckReport {
    /// The JSON contract version ([`REPORT_VERSION`]).
    pub report_version: u32,
    /// Core-conformance violations (cycles · unresolved refs · …).
    /// Non-empty ⇒ `waves` is empty and the secret analysis was skipped.
    pub conformance: Vec<ConformanceViolation>,
    /// Topological execution waves (`waves[n]` = task indices runnable
    /// once wave `n-1` completed). The plan. Empty when `conformance`
    /// has entries (no valid DAG order exists).
    pub waves: Vec<Vec<usize>>,
    /// Worst-case cost ceiling across all `infer:`/`agent:` tasks.
    pub cost: CostCeiling,
    /// The termination + parametric resource certificate (ADR-092 #7 ·
    /// AARA degree-1) — ALWAYS exists (acyclic + every loop/retry/turn
    /// capped makes termination a theorem of the language); the bounds
    /// are degree-1 polynomials in the `for_each` collection sizes.
    /// Additive: `report_version` stays 1.
    pub certificate: RunCertificate,
    /// Every `secrets.X` that escapes the masking boundary into an
    /// `exec`/`invoke` effect (directly, via a `with:` alias, or
    /// transitively through a tainted upstream output · IFC · ADR-092).
    pub secret_leaks: Vec<SecretLeak>,
    /// Every `secrets.X` that leaves the run as a workflow `outputs:`
    /// return value (the literal exfiltration · IFC egress · ADR-092).
    pub secret_egresses: Vec<SecretEgress>,
    /// Every statically-detectable effect outside the declared `permits:`
    /// boundary (empty when no `permits:` block is present).
    pub capability_escapes: Vec<CapabilityEscape>,
    /// Every deep `tasks.X.output.<path>` reference the declared shape
    /// (`schema:` / `output:` bindings) PROVES invalid — typo'd field
    /// names caught before a single token is spent (ADR-092 #4).
    pub schema_findings: Vec<SchemaTypeFinding>,
    /// Every `when:`-gate reachability finding — a PROVABLY dead task
    /// (the gate is unsatisfiable under every reachable combination of
    /// upstream terminal statuses) or a status comparison against a
    /// literal outside the spec vocabulary (`'failed'` for `'failure'`).
    /// ADR-092 ladder #6 (the acyclic no-SMT slice). Requires a valid
    /// DAG order — empty when `conformance` has entries.
    pub gate_findings: Vec<GateFinding>,
    /// Every `nika:` tool that names no canonical builtin (the closed
    /// 22-builtin catalog) — a runtime dispatch failure moved to check
    /// time, with the « did you mean » fix attached.
    pub unknown_tools: Vec<UnknownTool>,
    /// Every authored `schema:` defect that makes structured output
    /// unsatisfiable or un-compilable (required∉properties · bad `type`
    /// name · empty `enum`) — the static half of « structured output
    /// works in all cases ».
    pub schema_lints: Vec<SchemaLintFinding>,
    /// Advisory improvement hints (the deterministic « ameliorateur ») —
    /// each names the concrete change that unlocks a stronger static
    /// guarantee. NEVER fail the check ([`CheckReport::is_clean`]
    /// ignores them).
    pub hints: Vec<Hint>,
    /// The scheduler-independent DAG read — exact width (Dilworth) with
    /// a witness antichain, pinch points, per-task failure blast radius.
    /// `None` when conformance fails (no valid order) OR the workflow
    /// exceeds the exact-read size cap (analysis.rs · the O(n²) closure
    /// is a `DoS` surface at the parser's 10k-task limit) — both are the
    /// honest skip: no claim, never wrong. Additive: `report_version`
    /// stays 1.
    pub analysis: Option<DagAnalysis>,
}

impl CheckReport {
    /// Whether the workflow is clean — conformant, no leaks, no
    /// egresses, no capability escapes, no schema-type findings, no
    /// unknown tools, no schema-lint defects. (Cost-ceiling unknowns
    /// and hints are informational, not failures.)
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.conformance.is_empty()
            && self.secret_leaks.is_empty()
            && self.secret_egresses.is_empty()
            && self.capability_escapes.is_empty()
            && self.schema_findings.is_empty()
            && self.unknown_tools.is_empty()
            && self.schema_lints.is_empty()
            && self.gate_findings.is_empty()
    }
}

/// Run the full static pre-flight over a parsed workflow — INFALLIBLE
/// (the rustc model: maximal information per run).
///
/// Core conformance runs first. Its violations do not abort the check —
/// they land in [`CheckReport::conformance`] and every DAG-independent
/// analysis still runs, so an agent repairs conformance AND findings in
/// ONE round-trip. The plan (`waves`) and the IFC secret analysis need
/// a valid topological order and are skipped when conformance fails
/// (empty · documented on the fields).
#[must_use]
pub fn check(wf: &RawWorkflow) -> CheckReport {
    let (conformance, topo_waves) = match analyzer::analyze(wf) {
        Ok(AnalyzedWorkflow { topo_waves }) => (Vec::new(), topo_waves),
        Err(errors) => (
            errors
                .iter()
                .map(|e| ConformanceViolation {
                    code: e.spec_code().to_string(),
                    message: e.to_string(),
                    span: e.span().map(|s| ByteSpan {
                        start: s.start.0,
                        end: s.end.0,
                    }),
                })
                .collect(),
            Vec::new(),
        ),
    };
    // One IFC pass over the DAG — the taint fact base both leak reports
    // read. An empty wave order (conformance failure) taints nothing:
    // the analysis is simply skipped, never wrong.
    let flow = flow::analyze_flow(wf, &topo_waves);
    // The engineering read shares the same gating: a valid order or no
    // claim (its parallel-writers conflicts ride the hints, advisory).
    let dag_read = if conformance.is_empty() {
        analysis::read_dag(wf, &topo_waves)
    } else {
        analysis::DagRead::skipped()
    };
    let mut hints = hints::scan_hints(wf);
    hints.extend(dag_read.conflicts);
    CheckReport {
        report_version: REPORT_VERSION,
        conformance,
        cost: cost::ceiling(wf),
        certificate: certificate::certify(wf),
        secret_leaks: secrets::scan_leaks(wf, &flow),
        secret_egresses: secrets::scan_egresses(&flow),
        capability_escapes: permits_fit::scan_escapes(wf),
        schema_findings: schema_typing::scan_types(wf),
        unknown_tools: tools::scan_unknown_tools(wf),
        schema_lints: schema_lint::scan_schemas(wf),
        // gate reachability shares the IFC gating: a valid wave order or
        // nothing (skipped, never wrong)
        gate_findings: reach::scan_gates(wf, &topo_waves),
        hints,
        waves: topo_waves,
        analysis: dag_read.analysis,
    }
}

/// Infer the TIGHTEST `permits:` block the workflow actually needs —
/// capability inference (ADR-092 #2). Walks every task's literal effect
/// signature and synthesizes the minimal capability set; dynamic effects
/// (`${{ }}`-built paths/hosts/programs) widen their category and are
/// reported as honesty notes rather than silently dropped.
///
/// Unlike [`check`], this is pure synthesis — it never fails (a workflow
/// that does not analyze still has an inferable effect surface), so it
/// takes the parsed [`RawWorkflow`] and always returns a result.
#[must_use]
pub fn infer_permits(wf: &RawWorkflow) -> InferredPermits {
    infer_permits::infer(wf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn check_yaml(yaml: &str) -> CheckReport {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        check(&wf)
    }

    #[test]
    fn clean_minimal_workflow() {
        let r = check_yaml(
            "\
nika: v1
workflow: clean
tasks:
  - id: a
    exec: { command: \"echo hi\" }
",
        );
        assert!(r.is_clean());
        assert_eq!(r.waves, vec![vec![0]]);
    }

    #[test]
    fn repair_loop_converges_to_clean() {
        // The agent-loop contract, AUTOMATED: a 6-finding workflow's
        // emitted fixes/suggestions, applied verbatim, reach is_clean().
        // Round 1 — assert the exact repairs the report prescribes.
        let broken = r#"nika: v1
workflow: agent-demo
model: anthropic/claude-sonnet-4-6
permits:
  exec: false
  tools: ["nika:read"]
tasks:
  - id: extract
    infer:
      prompt: "extract"
      max_tokens: 200
      schema:
        type: object
        properties:
          summary: { type: string }
          score: { type: integre }
        required: [sumary]
  - id: save
    depends_on: [extract]
    invoke: { tool: "nika:wrte", args: { path: "./out.md", content: "${{ tasks.extract.output.sumarry }}" } }
  - id: push
    depends_on: [save]
    exec: { command: ["cargo", "publish"] }
"#;
        let wf = parse(broken, FileId::new(0), ParseMode::Strict).expect("parse");
        let r = check(&wf);
        assert!(!r.is_clean());
        // rename repairs (did-you-mean)
        assert_eq!(r.unknown_tools[0].suggestion.as_deref(), Some("nika:write"));
        assert!(
            r.schema_findings[0]
                .detail
                .contains("did you mean `summary`")
        );
        assert!(
            r.schema_lints
                .iter()
                .any(|l| l.detail.contains("`summary`"))
        );
        assert!(
            r.schema_lints
                .iter()
                .any(|l| l.detail.contains("`integer`"))
        );
        // grant repairs — ONE idiom, even against `exec: false`
        let fixes: Vec<&str> = r
            .capability_escapes
            .iter()
            .filter_map(|e| e.fix.as_deref())
            .collect();
        assert!(
            fixes.contains(&"add \"cargo\" to permits.exec"),
            "{fixes:?}"
        );

        // Round 2 — the workflow with every prescribed repair applied
        // (renames per suggestions · grants per fixes · the round-2 fs
        // grant surfaced once the tool rename lands).
        let repaired = broken
            .replace("sumary", "summary") // schema_lint suggestion
            .replace("integre", "integer") // schema_lint suggestion
            .replace("nika:wrte", "nika:write") // unknown_tools suggestion
            .replace("sumarry", "summary") // schema_findings suggestion
            .replace(
                "tools: [\"nika:read\"]",
                "tools: [\"nika:read\", \"nika:write\"]\n  fs: { write: [\"./out.md\"] }",
            )
            .replace("exec: false", "exec: [\"cargo\"]"); // add to a denying scalar
        let wf2 = parse(&repaired, FileId::new(0), ParseMode::Strict).expect("repaired parses");
        let r2 = check(&wf2);
        assert!(r2.is_clean(), "converged, but: {r2:?}");
    }

    #[test]
    fn core_violation_is_reported_in_band_with_partial_report() {
        // a cycle is a Core violation → reported IN the report (rustc model)
        let wf = parse(
            "\
nika: v1
workflow: cyclic
tasks:
  - id: a
    depends_on: [b]
    exec: { command: \"x\" }
  - id: b
    depends_on: [a]
    exec: { command: \"y\" }
",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("parse");
        let r = check(&wf);
        assert!(!r.is_clean(), "a cycle is a conformance violation");
        assert!(
            r.conformance.iter().any(|c| c.code == "NIKA-DAG-001"),
            "the cycle is reported in-band: {:?}",
            r.conformance
        );
        assert!(r.waves.is_empty(), "no valid order exists");
        // the rustc model: DAG-independent analyses STILL ran
        assert_eq!(r.report_version, REPORT_VERSION);
    }

    #[test]
    fn broken_dag_still_yields_every_dag_independent_finding() {
        // ONE round-trip: the agent gets the conformance violation AND
        // the tool typo AND the schema defect AND the hints, together.
        let src = "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    depends_on: [ghost]\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n  - id: b\n    infer:\n      prompt: \"x\"\n      schema:\n        type: object\n        properties:\n          s: { type: string }\n        required: [z]\n";
        let r = check_yaml(src);
        assert!(
            r.conformance.iter().any(|c| c.code == "NIKA-DAG-002"),
            "{:?}",
            r.conformance
        );
        // the violation carries its source span (the snippet renderers
        // + the LSP read it) — pinned to the EXACT token offsets so a
        // span() arm regressing to Default::default() cannot survive
        // (the mutation run proved the loose <1000 bound let it live)
        let dag = r
            .conformance
            .iter()
            .find(|c| c.code == "NIKA-DAG-002")
            .expect("dag violation");
        let span = dag.span.expect("span populated");
        let ghost = u32::try_from(src.find("ghost").expect("token")).expect("fits u32");
        assert_eq!(span.start, ghost, "span starts AT the offending token");
        // today's parser emits POINT spans for flow-list scalars
        // (end == start) — the snippet renderer handles zero-width;
        // upgrading scalars to full token ranges is the LSP-grade
        // follow-up, and this pin will catch the day it lands
        assert!(
            span.end >= span.start && span.end <= ghost + 5,
            "span anchored to the token: {span:?}"
        );
        assert_eq!(r.unknown_tools.len(), 1, "tool typo still caught");
        assert_eq!(r.schema_lints.len(), 1, "schema defect still caught");
        assert!(
            r.hints.iter().any(|h| h.kind == "cost"),
            "hints still computed: {:?}",
            r.hints
        );
        assert!(r.waves.is_empty() && r.secret_leaks.is_empty());
    }

    #[test]
    fn report_json_is_deterministic_across_runs() {
        // Two independent check() runs over the same input must render
        // byte-identical JSON — pins the BTree-everywhere discipline (a
        // stray HashMap would randomize field/finding order run-to-run).
        let yaml = "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\npermits: { exec: false, tools: [\"nika:read\"] }\nsecrets:\n  k: { source: vault, key: x }\ntasks:\n  - id: a\n    invoke: { tool: \"nika:raed\", args: { path: \"./in\" } }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"curl -d ${{ secrets.k }} x\" }\n  - id: c\n    depends_on: [b]\n    infer: { prompt: \"go ${{ tasks.b.output }}\", max_tokens: 50 }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let first = serde_json::to_string(&check(&wf)).expect("serialize");
        let second = serde_json::to_string(&check(&wf)).expect("serialize");
        assert_eq!(first, second, "same input → byte-identical report");
    }
}
