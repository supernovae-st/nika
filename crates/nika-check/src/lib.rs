// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-check` — the static judgment crate: the workflow **analyzer**
//! (Core conformance · the derived DAG edges) plus the `nika check`
//! pre-flight ladder (audit-before-it-runs).
//!
//! Split from `nika-schema` 2026-07-21 at the 15k crate-size wall (the
//! nika-graph/nika-dap precedents): the parser keeps its blueprint shape
//! (`nika-schema` = AST + parser · L0), the judgment over the parsed AST
//! lives HERE — pure, sync, zero I/O, zero async (L0).
//!
//! Because the language is statically analyzable BY CONSTRUCTION (acyclic
//! DAG · bounded `for_each` · non-Turing CEL · declared effects), this
//! crate answers « what will this workflow do, cost, and touch? » with
//! **zero API calls and zero tokens spent** — the property no other AI
//! workflow runner gives (spec `07-conformance.md` §`nika check`).
//!
//! It composes [`analyze`] (Core conformance) with three
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

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
        clippy::manual_string_new,
        clippy::panic,
        clippy::unreachable,
    )
)]

pub mod analyzer;

mod analysis;
mod certificate;
mod composition;
mod content_flow;
mod cost;
mod declass;
mod effective;
mod findings;
mod flow;
mod hints;
pub mod native_first;
pub mod permits_fit;
mod permits_infer;
mod policy;
mod reach;
mod requirements;
mod schema_lint;
mod schema_typing;
mod secrets;
mod tools;
pub mod trifecta;

use nika_schema::error::{SpecCategory, SpecCode};
use nika_schema::raw::RawWorkflow;

pub use analysis::{DagAnalysis, TaskBlast};
pub use certificate::{Bound, CertTerm, RunCertificate};
pub use composition::CompositionFinding;
pub use cost::{CostCeiling, TaskCost, UnboundedReason};
pub use effective::{EffectivePermits, PermitsSource};
pub use findings::UnifiedFinding;
pub use flow::{FlowFacts, TaintTrace};
pub use hints::{Hint, static_read_paths};
pub use permits_fit::CapabilityEscape;
pub use permits_infer::InferredPermits;
pub use reach::{GateFinding, GateFindingKind, STATUS_VOCAB};
pub use requirements::{ModelRequirement, Requirements, SecretRequirement};
pub use schema_lint::SchemaLintFinding;
pub use schema_typing::SchemaTypeFinding;
pub use secrets::{SecretEgress, SecretLeak};
pub use tools::{MissingArg, UnknownArg, UnknownTool};

// The analyzer's surface at the crate root — the same shape `nika-schema`
// re-exported before the split (`analyze` · `AnalyzedWorkflow` · the
// type-contract projections).
pub use analyzer::{AnalyzedWorkflow, analyze, lowered_returns, named_types, returns_type};

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

// The base URL of the per-code error pages (`<base>/<CODE>`) — DEFINED in
// `nika_schema::error::spec_code` (one voice beside `SpecCode`; the parser
// side's `skill.rs` stamps the same URL, so the constant must live BELOW
// this crate — never the reverse edge) and re-exported here so the check
// surface keeps its historical name.
pub use nika_schema::error::ERROR_DOCS_BASE;

/// A finding's severity, stamped BY the engine (the one truth — no
/// consumer re-derives it from the code or the family). `lowercase`
/// on the wire (`"error"`). Additive: `report_version` stays 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum FindingSeverity {
    /// The workflow will not run correctly — fails `is_clean`.
    Error,
    /// Suspicious but runnable (reserved — no warning-grade
    /// conformance rule exists today).
    Warning,
    /// Advisory (reserved for future finding families).
    Info,
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
    /// Engine-stamped severity — conformance violations are always
    /// [`FindingSeverity::Error`] (they block the DAG). Additive:
    /// `report_version` stays 1.
    pub severity: FindingSeverity,
    /// The per-code documentation page
    /// (`https://nika.sh/errors/<CODE>` · [`ERROR_DOCS_BASE`]) —
    /// editors surface the code as a clickable link. Additive:
    /// `report_version` stays 1.
    pub docs_url: String,
    /// The exact offending source token, when the violation is a
    /// RENAME-shaped defect (`buidl` for an unknown dependency ·
    /// `tasks.buidl` for an unresolved reference) — paired with
    /// [`Self::suggestion`], this is the machine-applicable half the
    /// human message already renders as « did you mean ___? ». The
    /// `--fix` repair loop splices exactly this token. Additive:
    /// `report_version` stays 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offending: Option<String>,
    /// The typed rename target (`build` · `tasks.build`) — present only
    /// when the deterministic did-you-mean asserted one (silence past
    /// the threshold, as everywhere). Additive: `report_version` stays 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
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
    /// The AFFIRMATIVE permits statement — the boundary in force (the
    /// declared `permits:` block or the engine floor) AND the tightest
    /// boundary the body statically needs (the `--infer-permits`
    /// derivation) — so consumers render the positive contract on a
    /// green check instead of reconstructing it from graph labels;
    /// violations stay in `capability_escapes` / `secret_*`. Additive:
    /// `report_version` stays 1.
    pub permits: EffectivePermits,
    /// What this workflow needs from its caller BEFORE any token is
    /// spent — models per task · declared secrets (facts, no values) ·
    /// env reads vs env defines · required vars. Declaration truth
    /// only: presence checks stay with the caller (a static report
    /// never depends on who runs it). Additive: `report_version`
    /// stays 1.
    pub requirements: Requirements,
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
    /// Every hard `policy:` rule violation (spec 10 · `NIKA-POLICY-001` —
    /// judged on the derived graph, so empty when `conformance` has
    /// entries). Additive: `report_version` stays 1.
    pub policy_findings: Vec<nika_cap::PolicyViolation>,
    /// Every lethal-trifecta finding (NEP-0002 · `NIKA-SEC-009`): all
    /// three legs declared AND an egress-capable task no blocking
    /// `nika:prompt` gate dominates. Judged on the derived graph and the
    /// declared boundary — empty when `conformance` has entries OR no
    /// `permits:` block is declared (no claim, never wrong). Additive:
    /// `report_version` stays 1.
    pub trifecta_findings: Vec<nika_cap::TrifectaViolation>,
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
    /// stdlib catalog — the count lives in `nika-builtin`, never here) —
    /// a runtime dispatch failure moved to check time, with the
    /// « did you mean » fix attached.
    pub unknown_tools: Vec<UnknownTool>,
    /// Every `invoke` call passing an `args:` key the named builtin does
    /// not declare — the silent-footgun class (`nika:jq` with `data:`
    /// instead of `input:` runs over `null` and returns `null`, no error),
    /// surfaced at check time with the « did you mean » fix.
    pub unknown_args: Vec<UnknownArg>,
    /// Every `invoke` call MISSING an unconditionally-required `args:` key
    /// (the `Builtin::required` set). Closes the « only a handful of
    /// builtins had a static required-arg check » gap — a required-arg
    /// builtin now fails
    /// `nika check` instead of passing `check {}` and failing at run. The
    /// conditional contracts (`nika:wait`, `nika:fetch`) stay in
    /// `conformance` (the `builtin_shape` ladder) — no double report.
    pub missing_args: Vec<MissingArg>,
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
    /// The composition lane (spec 14 · `NIKA-COMP-001..004`). A plain
    /// [`check`] carries the PURE half (templated/malformed/unpinned
    /// targets — no filesystem needed); [`check_composed`] adds the
    /// resolved half (cycles · containment · the typed call) through an
    /// injected reader. Additive: `report_version` stays 1.
    pub composition: Vec<CompositionFinding>,
    /// Every finding, class-erased into ONE renderable list (#331) —
    /// the per-class keys above stay (the typed surface); a consumer
    /// loops this instead of hardcoding ten key names. Empty ⇔
    /// [`CheckReport::is_clean`] (the completeness ratchet in
    /// `findings::tests`). Additive: `report_version` stays 1.
    pub findings: Vec<UnifiedFinding>,
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
            && self.policy_findings.is_empty()
            && self.trifecta_findings.is_empty()
            && self.schema_findings.is_empty()
            && self.unknown_tools.is_empty()
            && self.unknown_args.is_empty()
            && self.missing_args.is_empty()
            && self.schema_lints.is_empty()
            && self.gate_findings.is_empty()
            && self.composition.is_empty()
    }

    /// The spec codes from the CHECK-ONLY finding surfaces — those NOT in
    /// [`Self::conformance`] (which the deep `analyze` tier already yields).
    ///
    /// The conformance suite is the `nika check` surface (the real surface an
    /// author runs); its harness verdicts against `analyze()` (the conformance
    /// tier) UNION this (the builtin-arg + capability-boundary tier `check()`
    /// adds), so a fixture like « `nika:write` without `content` » (caught by
    /// `missing_args`, not by `analyze`) is tested. Mirrors the spec's
    /// namespace allocation: builtin arg-contract violations → `NIKA-BUILTIN`,
    /// a body outside the declared `permits:` → `NIKA-SEC-004`, a hard
    /// `policy:` rule violation → `NIKA-POLICY-001` (spec 10 · the
    /// `core/policy` fixtures match on it).
    #[must_use]
    pub fn extra_conformance_codes(&self) -> Vec<SpecCode> {
        let builtin = SpecCode::new("BUILTIN", 1, SpecCategory::ValidationError);
        let mut codes = Vec::new();
        codes.extend(self.capability_escapes.iter().map(|e| {
            // Floor escapes carry the code the run would emit (SEC-005 ·
            // the always-on SSRF floor); boundary escapes stay SEC-004.
            SpecCode::new(
                "SEC",
                if e.floor { 5 } else { 4 },
                SpecCategory::SecurityError,
            )
        }));
        // Hard policy: violations (spec 10) → NIKA-POLICY-001.
        let policy_code = SpecCode::new("POLICY", 1, SpecCategory::SecurityError);
        codes.extend(self.policy_findings.iter().map(|_| policy_code));
        // Lethal trifecta (NEP-0002) → NIKA-SEC-009.
        let trifecta_code = SpecCode::new("SEC", 9, SpecCategory::SecurityError);
        codes.extend(self.trifecta_findings.iter().map(|_| trifecta_code));
        codes.extend(self.unknown_tools.iter().map(|_| builtin));
        codes.extend(self.unknown_args.iter().map(|_| builtin));
        codes.extend(self.missing_args.iter().map(|_| builtin));
        // Gate liveness (03 §static liveness · check-only, reach.rs):
        // DAG-006 statically dead task · DAG-007 out-of-vocabulary literal.
        codes.extend(self.gate_findings.iter().map(|g| match g.kind {
            reach::GateFindingKind::DeadTask => {
                SpecCode::new("DAG", 6, SpecCategory::ValidationError)
            }
            reach::GateFindingKind::BadStatusLiteral => {
                SpecCode::new("DAG", 7, SpecCategory::ValidationError)
            }
        }));
        // Composition lane (spec 14): COMP-002 is the security law
        // (child boundary ⊄ parent); 001/003/004 are validation.
        codes.extend(self.composition.iter().map(|f| match f.code {
            "NIKA-COMP-002" => SpecCode::new("COMP", 2, SpecCategory::SecurityError),
            "NIKA-COMP-003" => SpecCode::new("COMP", 3, SpecCategory::ValidationError),
            "NIKA-COMP-004" => SpecCode::new("COMP", 4, SpecCategory::ValidationError),
            _ => SpecCode::new("COMP", 1, SpecCategory::ValidationError),
        }));
        codes
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
    let (conformance, topo_waves, edges) = match analyzer::analyze(wf) {
        Ok(AnalyzedWorkflow {
            topo_waves, edges, ..
        }) => (Vec::new(), topo_waves, edges),
        Err(errors) => (
            errors
                .iter()
                .map(|e| {
                    let code = e.spec_code().to_string();
                    let docs_url = format!("{ERROR_DOCS_BASE}/{code}");
                    let (offending, suggestion) = match e.rename_repair() {
                        Some((o, s)) => (Some(o), Some(s)),
                        None => (None, None),
                    };
                    ConformanceViolation {
                        code,
                        message: e.to_string(),
                        span: e.span().map(|s| ByteSpan {
                            start: s.start.0,
                            end: s.end.0,
                        }),
                        severity: FindingSeverity::Error,
                        docs_url,
                        offending,
                        suggestion,
                    }
                })
                .collect(),
            Vec::new(),
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
    hints.extend(native_first::scan(wf));
    hints.extend(dag_read.conflicts);
    // policy reads graph ancestors — valid order or no claim (IFC gating)
    let policy_findings = if conformance.is_empty() {
        policy::scan_policy(wf, &edges)
    } else {
        Vec::new()
    };
    // The trifecta lane shares the gating (NEP-0002 · valid DAG or no claim).
    let trifecta_findings = if conformance.is_empty() {
        trifecta::scan_trifecta(wf, &edges, &topo_waves)
    } else {
        Vec::new()
    };
    let mut report = CheckReport {
        report_version: REPORT_VERSION,
        conformance,
        cost: cost::ceiling(wf),
        certificate: certificate::certify(wf),
        requirements: requirements::collect(wf),
        permits: effective::collect(wf),
        secret_leaks: secrets::scan_leaks(wf, &flow),
        secret_egresses: secrets::scan_egresses(&flow),
        capability_escapes: permits_fit::scan_escapes(wf),
        policy_findings,
        trifecta_findings,
        schema_findings: schema_typing::scan_types(wf),
        unknown_tools: tools::scan_unknown_tools(wf),
        unknown_args: tools::scan_unknown_args(wf),
        missing_args: tools::scan_missing_args(wf),
        schema_lints: schema_lint::scan_schemas(wf),
        // gate reachability shares the IFC gating: a valid wave order or
        // nothing (skipped, never wrong)
        gate_findings: reach::scan_gates(wf, &topo_waves, &edges),
        // the PURE composition half (spec 14 law 1's textual part);
        // the resolved half needs a reader — `check_composed`
        composition: composition::scan_static(wf),
        hints,
        waves: topo_waves,
        analysis: dag_read.analysis,
        findings: Vec::new(),
    };
    // The class-erased list folds the FINISHED report (one truth, read
    // back) — every consumer (CLI --json · MCP nika_check) gets it free.
    report.findings = findings::collect(&report);
    report
}

/// [`check`] + the RESOLVED composition lane (spec 14): the call graph
/// is walked through the injected reader (the [`nika_schema::resolve_skills`]
/// pattern — this crate stays zero-I/O), judging acyclicity
/// (`NIKA-COMP-003`), effect containment (`NIKA-COMP-002`) and the
/// typed call (`NIKA-COMP-004`) on top of the pure target checks
/// (`NIKA-COMP-001`). `root` is the workflow's own id/path (relative
/// child targets resolve against its directory).
///
/// The CLI's check/run gates call THIS; a reader-less surface calling
/// plain [`check`] still gets the pure half — one voice, two depths.
#[must_use]
pub fn check_composed(
    wf: &RawWorkflow,
    root: &str,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
) -> CheckReport {
    let mut report = check(wf);
    report.composition = composition::scan_resolved(wf, root, read);
    // Re-fold the class-erased list over the finished lane (one truth).
    report.findings = findings::collect(&report);
    report
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
    permits_infer::infer(wf)
}

/// ONE task's capability attribution as deterministic permit strings —
/// the `graph --format json` node projector (`exec:` · `fs.read:` ·
/// `fs.write:` · `net.http:` · `tool:` families, BTree-ordered). Same
/// effect walk as [`infer_permits`], un-aggregated.
#[must_use]
pub fn task_permits(task: &nika_schema::raw::RawTask) -> Vec<String> {
    permits_infer::task_permits(task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn check_yaml(yaml: &str) -> CheckReport {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        check(&wf)
    }

    #[test]
    fn conformance_violations_carry_severity_and_docs_url() {
        // An unresolved dependency -> a conformance violation. The finding
        // must stamp its own severity AND the canonical docs URL so every
        // consumer (extension diagnostics, agent loops, CI) reads ONE
        // truth without re-deriving either.
        let r = check_yaml(
            "\
nika: v1
workflow:
  id: t
tasks:
  a:
    after: { ghost: success }
    exec: { command: [\"echo\", \"hi\"] }
",
        );
        assert!(!r.conformance.is_empty());
        let v = &r.conformance[0];
        assert_eq!(v.severity, FindingSeverity::Error);
        assert_eq!(v.docs_url, format!("{ERROR_DOCS_BASE}/{}", v.code));

        // The wire form: lowercase severity, absolute per-code URL.
        let json = serde_json::to_value(&r).expect("report serializes");
        let c = &json["conformance"][0];
        assert_eq!(c["severity"], "error");
        assert!(
            c["docs_url"]
                .as_str()
                .expect("docs_url is a string")
                .starts_with("https://nika.sh/errors/NIKA-"),
        );
    }

    #[test]
    fn clean_minimal_workflow() {
        let r = check_yaml(
            "\
nika: v1
workflow:
  id: clean
tasks:
  a:
    exec: { command: [\"echo\", \"hi\"] }
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
workflow:
  id: agent-demo
model: anthropic/claude-sonnet-4-6
permits:
  exec: false
  tools: ["nika:read"]
tasks:
  extract:
    infer:
      prompt: "extract"
      max_tokens: 200
      schema:
        type: object
        properties:
          summary: { type: string }
          score: { type: integre }
        required: [sumary]
  save:
    with: { content: "${{ tasks.extract.output.sumarry }}" }
    invoke: { tool: "nika:wrte", args: { path: "./out.md", content: "${{ with.content }}" } }
  push:
    after: { save: success }
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
workflow:
  id: cyclic
tasks:
  a:
    after: { b: success }
    exec: { command: [\"x\"] }
  b:
    after: { a: success }
    exec: { command: [\"y\"] }
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
        let src = "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    after: { ghost: success }\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n  b:\n    infer:\n      prompt: \"x\"\n      schema:\n        type: object\n        properties:\n          s: { type: string }\n        required: [z]\n";
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
        let yaml = "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\npermits: { exec: false, tools: [\"nika:read\"] }\nsecrets:\n  k: { source: vault, key: x }\ntasks:\n  a:\n    invoke: { tool: \"nika:raed\", args: { path: \"./in\" } }\n  b:\n    after: { a: success }\n    exec: { command: [\"curl\", \"-d\", \"${{ secrets.k }}\", \"x\"] }\n  c:\n    with: { b_out: \"${{ tasks.b.output }}\" }\n    infer: { prompt: \"go ${{ with.b_out }}\", max_tokens: 50 }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let first = serde_json::to_string(&check(&wf)).expect("serialize");
        let second = serde_json::to_string(&check(&wf)).expect("serialize");
        assert_eq!(first, second, "same input → byte-identical report");
    }

    // ── extra_conformance_codes · the CHECK-ONLY finding → spec-code map ─
    //
    // These pin every arm of `extra_conformance_codes` so the `-> vec![]`
    // mutation (and any per-surface arm removal) fails: a report carrying
    // findings must yield the matching codes, a clean report must yield
    // NONE, and the count must equal the total findings (one code per
    // finding, no surface dropped).

    #[test]
    fn extra_conformance_codes_empty_on_clean_report() {
        // The negative anchor: a clean workflow has no CHECK-ONLY findings,
        // so the code list is empty. Paired with the non-empty tests below
        // this distinguishes the REAL fn (state-dependent) from the
        // `-> vec![]` mutant (always-empty) — the mutant passes THIS test
        // but fails the populated ones.
        let r = check_yaml(
            "\
nika: v1
workflow:
  id: clean
tasks:
  a:
    exec: { command: [\"echo\", \"hi\"] }
",
        );
        assert!(r.is_clean());
        assert!(
            r.extra_conformance_codes().is_empty(),
            "a clean report yields no extra codes: {:?}",
            r.extra_conformance_codes(),
        );
    }

    #[test]
    fn extra_conformance_codes_maps_capability_escape_to_sec_004() {
        // A body outside the declared `permits:` → NIKA-SEC-004
        // (the capability_escapes arm). This kills `-> vec![]` AND the
        // capability-escape arm removal (without it the list is empty).
        let r = check_yaml(
            "\
nika: v1
workflow:
  id: escape
permits:
  exec: false
tasks:
  a:
    exec: { command: [\"cargo\", \"publish\"] }
",
        );
        assert!(
            !r.capability_escapes.is_empty(),
            "the fixture must produce a capability escape",
        );
        let codes = r.extra_conformance_codes();
        assert!(
            !codes.is_empty(),
            "a capability escape must surface a code (not vec![])",
        );
        let rendered: Vec<String> = codes.iter().map(ToString::to_string).collect();
        assert!(
            rendered.iter().any(|c| c == "NIKA-SEC-004"),
            "capability escape → NIKA-SEC-004: {rendered:?}",
        );
        // pin the category too — the arm constructs SecurityError, not the
        // generic ValidationError the builtin arm uses
        assert!(
            codes
                .iter()
                .any(|c| c.category == SpecCategory::SecurityError),
            "the SEC-004 arm is a SecurityError: {codes:?}",
        );
    }

    #[test]
    fn extra_conformance_codes_maps_unknown_tool_to_builtin_001() {
        // A typo'd `nika:` builtin → NIKA-BUILTIN-001 (the unknown_tools
        // arm). Conformant DAG so ONLY the builtin surface fires — pins
        // the unknown_tools arm in isolation.
        let r = check_yaml(
            "\
nika: v1
workflow:
  id: typo
tasks:
  a:
    invoke: { tool: \"nika:wrte\", args: { path: \"./out\", content: \"x\" } }
",
        );
        assert!(
            !r.unknown_tools.is_empty(),
            "the fixture must produce an unknown tool",
        );
        let codes = r.extra_conformance_codes();
        let rendered: Vec<String> = codes.iter().map(ToString::to_string).collect();
        assert!(
            rendered.iter().any(|c| c == "NIKA-BUILTIN-001"),
            "unknown tool → NIKA-BUILTIN-001: {rendered:?}",
        );
        assert!(
            codes
                .iter()
                .any(|c| c.category == SpecCategory::ValidationError),
            "the BUILTIN arm is a ValidationError: {codes:?}",
        );
    }

    #[test]
    fn extra_conformance_codes_count_equals_total_findings() {
        // One code PER finding, every surface counted. Pins the
        // `extend` chain wholesale: dropping any of the four arms
        // (capability_escapes · unknown_tools · unknown_args ·
        // missing_args) would shrink the count below the sum and fail.
        let r = check_yaml(
            "\
nika: v1
workflow:
  id: many
permits:
  exec: false
  tools: [\"nika:read\"]
tasks:
  a:
    exec: { command: [\"cargo\"] }
  b:
    invoke: { tool: \"nika:wrte\", args: { path: \"./out\", content: \"x\" } }
",
        );
        let expected = r.capability_escapes.len()
            + r.policy_findings.len()
            + r.unknown_tools.len()
            + r.unknown_args.len()
            + r.missing_args.len();
        assert!(
            expected >= 2,
            "the fixture must produce ≥2 findings: {expected}"
        );
        assert_eq!(
            r.extra_conformance_codes().len(),
            expected,
            "exactly one code per CHECK-ONLY finding across all four surfaces",
        );
    }
}
