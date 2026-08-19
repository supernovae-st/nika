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
//!
//! # The two laws every rung in here obeys
//!
//! Both were paid for. A 2026-07-28/29 audit of this crate and its
//! runtime counterpart found 23 defects across three domains, and they
//! were not 23 mistakes — they were two mistakes made repeatedly.
//!
//! ## 1 · Cover the claim, or narrow the claim to what you cover
//!
//! A rung's sentence is a promise, and a green that means less than it
//! says is worse than no green: it spends the reader's trust and returns
//! nothing. Measured instances, all shipped, all now narrowed —
//!
//! - `TYPES` said every deep reference fits its declared shape. No
//!   builtin can declare an output shape, so references into builtin
//!   output were UNCHECKED, not checked-and-fine.
//! - `COST` said *worst-case spend* while pricing `max_tokens`, which the
//!   spec defines as max OUTPUT tokens. On the commonest first workflow —
//!   fetch a document, summarise it — a 3.2 MB body interpolated into one
//!   prompt is ~818k input tokens and $2.46, under a green line reading
//!   $0.0075. 328×.
//! - `PERMITS` said the body fits the declared boundary while judging
//!   literal arguments only, so a `const:`-backed path was invisible.
//!
//! The repair is usually in the words, not the machinery. When you cannot
//! widen the coverage, narrow the sentence and NAME what defers — that
//! option is always available and always correct.
//!
//! ## 2 · An undecidable question almost always contains a decidable one
//!
//! > When a proof obligation is waived as undecidable, NAME the decision
//! > procedure it would have needed. If you can write it down, the waiver
//! > is a hole.
//!
//! Four instances, three rungs, three independent authors —
//!
//! - The fs-boundary differential excluded mid-pattern globs, citing
//!   *"glob-pattern ⊆ permits-glob inclusion is not soundly decidable"*.
//!   True, and about containment between two PATTERNS; both sides matched
//!   a CONCRETE path against ONE pattern, which is ordinary glob matching.
//!   **The waiver is where a shipped fail-open lived**: `data/*.csv`
//!   granted `data/**`, and a permit naming CSV files read a private key
//!   three directories down.
//! - A `nika:notify` host lives inside a secret, so the whole question
//!   looked closed. *Which* host is unknowable; *is there ANY host* is a
//!   set-emptiness test, and an empty `net.http` makes the run certainly
//!   fail.
//! - A tool-authority conjunct was dropped along with a dynamic argument
//!   it never depended on.
//!
//! It recurs because a true impossibility result FEELS like a complete
//! answer, so the search stops there. It is not carelessness — every one
//! was written by someone who had the theory right. The prompt that finds
//! these is not "be careful", it is:
//!
//! > *What is the strongest claim I CAN decide here, and does the code
//! > make it?*
//!
//! **So: a comment in this crate that waives a check must name the
//! sub-question it considered and why that one does not survive either.**
//! A waiver with no named alternative is reviewable as incomplete, and
//! four times out of four the alternative existed.
//!
//! Full record, with every repro: `docs/plans/2026-07-28-verdict-coverage.md`.

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

/// The analysis substrate, re-exported at its historical path.
///
/// It descended to the `nika-check-analyzer` member at the 15k prod-LOC wall
/// (ONE architectural unit, TWO workspace members — D-2026-07-09-N1 · the
/// ADR-110 pattern). Every call site keeps writing `nika_check::analyzer::…`
/// and `crate::analyzer::…`: the boundary moved, the surface did not.
pub use nika_check_analyzer as analyzer;

mod analysis;
mod certificate;
mod composition;
mod conformance_codes;
mod consent;
mod content_flow;
mod cost;
mod data_journey;
mod data_sink;
mod declass;
mod effective;
mod energy;
mod exec_floor;
mod findings;
mod flow;
mod hints;
mod lift;
pub mod native_first;
mod order;
mod permit_taint;
pub mod permits_fit;
mod permits_infer;
mod reach;
mod requirements;
mod risk;
mod run_decl;
mod schema_lint;
mod schema_typing;
mod secrets;
mod tools;
pub mod trifecta;
mod walk;

use nika_schema::error::SpecCode;
use nika_schema::raw::RawWorkflow;

pub use analysis::{DagAnalysis, TaskBlast, WriteConflict};
pub use certificate::{Bound, CertTerm, RunCertificate};
pub use composition::CompositionFinding;
pub use consent::ConsentFinding;
pub use cost::{ComposedCost, CostCeiling, TaskCost, UnboundedReason};
pub use data_journey::{
    DataClassification, DataJourney, EndpointLocus, JourneyConsent, JourneyEndpoint, ModelEndpoint,
    RetentionFact, SecretUse,
};
pub use data_sink::SinkFinding;
pub use declass::LeakReason;
pub use effective::{EffectivePermits, PermitsSource};
pub use energy::{EnergyCounts, EnergyReading, EnergyTask};
pub use exec_floor::ExecFloorFinding;
pub use findings::UnifiedFinding;
pub use flow::{FlowFacts, TaintTrace, action_effect_fields};
pub use hints::{Hint, PAID_RUN_KINDS, compiled, paid_blockers, paid_ready, stamp_paid_ready};
pub use lift::LiftFinding;
pub use order::OrderFinding;
pub use permit_taint::{PermitTaint, PermitTaintKind};
pub use permits_fit::CapabilityEscape;
pub use permits_infer::InferredPermits;
pub use reach::{GateFinding, GateFindingKind, STATUS_VOCAB};
pub use requirements::{ModelRequirement, Requirements, SecretRequirement};
pub use risk::{RiskGrade, risk_grade};
pub use run_decl::RunDeclFinding;
pub use schema_lint::SchemaLintFinding;
pub use schema_typing::{SchemaTypeFinding, UnverifiableOutputRef};
pub use secrets::{SecretEgress, SecretLeak};
pub use tools::{MissingArg, UnknownArg, UnknownTool};
pub use walk::{static_literal_of, static_read_paths};

// The analyzer's surface at the crate root — the same shape `nika-schema`
// re-exported before the split (`analyze` · `AnalyzedWorkflow` · the
// type-contract projections).
pub use analyzer::{AnalyzedWorkflow, analyze, lowered_returns, returns_type};

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
    /// (`https://nika.sh/language/errors/<CODE>` · [`ERROR_DOCS_BASE`]) —
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
    /// The ENERGY reading (NEP-0018) — the cost honesty transposed to
    /// watt-hours over the catalog's sourced figures (the static half;
    /// the CLI's check renderer reads it). Additive: `report_version`
    /// stays 1.
    pub energy: EnergyReading,
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
    /// The DATA JOURNEY (P0-18) — where this workflow's data goes,
    /// projected from the facts above: sources · destinations · model
    /// endpoints · the secrets in play (NAMES, never values — law 13) ·
    /// the declared clearances · the derived classification. Advisory by
    /// design: the blocking refusals stay in their own lanes (`secret_*`
    /// · `consent_findings`); the journey makes every cloud flow VISIBLE
    /// so no sensitive sink ever rides without a receipt the operator
    /// has seen. Additive: `report_version` stays 1.
    pub data_journey: DataJourney,
    /// Every `secrets.X` that escapes the masking boundary into an
    /// `exec`/`invoke` effect (directly, via a `with:` alias, or
    /// transitively through a tainted upstream output · IFC · ADR-092).
    pub secret_leaks: Vec<SecretLeak>,
    /// Every `secrets.X` that leaves the run as a workflow `outputs:`
    /// return value (the literal exfiltration · IFC egress · ADR-092).
    pub secret_egresses: Vec<SecretEgress>,
    /// Every statically-detectable effect outside the declared `permits:`
    /// boundary. F-O8 « absent = zero authority »: with NO `permits:`
    /// block every effect escapes against the zero boundary (flagged
    /// `undeclared` → `NIKA-AUTH-006`) — only a pure-compute body stays
    /// empty here (and gets the « declare `permits: {}` » hint instead).
    pub capability_escapes: Vec<CapabilityEscape>,
    /// Every argv-form `exec:` command the runtime's exec floor WILL
    /// refuse (#605 · `NIKA-SEC-001`) — judged by the SAME predicate the
    /// run uses ([`nika_types::exec::argv_floor_refusal`]), so check ≡
    /// run: a literal `["bash","-c",…]` no longer audits green and dies
    /// at spawn (and an `on_error: skip` leg can no longer hide it).
    /// Literal argv only — a `${{ }}` island defers to the runtime's
    /// pre-spawn re-judgment. Additive: `report_version` stays 1.
    pub exec_floor_findings: Vec<ExecFloorFinding>,
    /// Every permit-parameterization taint finding (NEP-0004 · the static
    /// twin of the runtime re-gate): an interpolated permit BOUND
    /// (`NIKA-AUTH-007` · law 1) or an untrusted value whose canonical
    /// resolved form escapes the step's permit (`NIKA-AUTH-008` · law 2).
    /// Judged under a PRESENT block only; unresolvable untrusted values
    /// defer to the runtime `NIKA-SEC-004` (law 4). Additive:
    /// `report_version` stays 1.
    pub permit_taints: Vec<PermitTaint>,
    /// Every data-as-code sink finding (NEP-0006 · `NIKA-SEC-008`): a
    /// `nika:fetch` whose resolved URL path names a code-bearing class
    /// with no `inert:` door declared. The unresolvable defers to the
    /// run twin (`NIKA-SEC-004` · law 3). Additive: `report_version`
    /// stays 1.
    pub sink_findings: Vec<SinkFinding>,
    /// Every affirmative-consent refusal (NEP-0020 · `NIKA-SEC-014`): an
    /// egress-capable task reached from a confirm-mode human gate over a
    /// route no affirmative gate closes — false triggers exactly zero
    /// effects. Judged on the derived graph — empty when `conformance`
    /// has entries. The UNDECIDABLE remainder (a nested binding · a
    /// non-fragment gate) stays advisory in `hints` — the blocking row
    /// fires only on the PROVEN route (sound, never a false red).
    /// Additive: `report_version` stays 1.
    pub consent_findings: Vec<ConsentFinding>,
    /// Every order-law refusal (spec 10 · `NIKA-SEC-015`): an `exec:`
    /// task transitively downstream of a net-effecting one over the
    /// derived graph. UNCONDITIONAL — no block declares it and none
    /// can disable it; the only gate is a valid DAG, because a graph
    /// that did not build cannot be walked. Additive:
    /// `report_version` stays 1.
    pub order_findings: Vec<OrderFinding>,
    /// Every authored door that guards nothing (spec 10 §the authored
    /// doors rule 6 · `NIKA-AUTH-011`): a well-shaped `lift:` naming a
    /// law that would never have fired on its task. Each arm ASKS the
    /// law it judges rather than re-typing its conditions. Additive:
    /// `report_version` stays 1.
    pub lift_findings: Vec<LiftFinding>,
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
    /// Every deep output reference the lane CANNOT judge (F3 ·
    /// 2026-07-30): the target task exists but declares no output shape
    /// (a builtin invoke without `returns:` · an exec without `output:`
    /// bindings). Never a finding — the verdict line counts them so the
    /// ✔ names its own blind spot. Additive: `report_version` stays 1.
    pub unverifiable_output_refs: Vec<UnverifiableOutputRef>,
    /// Every `when:`-gate reachability finding — a PROVABLY dead task
    /// (the gate is unsatisfiable under every reachable combination of
    /// upstream terminal statuses) or a status comparison against a
    /// literal outside the spec vocabulary (`'failed'` for `'failure'`).
    /// ADR-092 ladder #6 (the acyclic no-SMT slice). Requires a valid
    /// DAG order — empty when `conformance` has entries.
    pub gate_findings: Vec<GateFinding>,
    /// Every `run:` declaration the body contradicts (F-P3) —
    /// `entropy: none` demands strict determinism but a structural
    /// entropy source is used (a `retry:` jitter · the non-hermetic
    /// `nika:uuid` builtin). Additive: `report_version` stays 1.
    pub run_decl_findings: Vec<RunDeclFinding>,
    /// Every write-write conflict (F-P15 · NEP-0014 law 1 ·
    /// `NIKA-SEC-012`): two tasks incomparable in the DAG closure whose
    /// literal `nika:write` paths collide, or a `for_each` fan writing
    /// one constant path — the last-writer-wins race, refused (an
    /// ordering edge discharges it). Judged on the derived graph —
    /// empty when `conformance` has entries. Additive:
    /// `report_version` stays 1.
    pub write_conflicts: Vec<WriteConflict>,
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
    /// The semantic hash (the proof layer's Merkle root) of the workflow
    /// this report JUDGED — the judged-vs-booted binding (F-P2). The
    /// stamp is applied by the producer that owns the hash machinery:
    /// `check()` stays pure over nika-schema types (the semantic hash
    /// lives in nika-runtime, which depends on THIS crate — computing it
    /// here would close a dependency cycle), so the CLI's load seam
    /// stamps it. `None` = unstamped: the runtime's trust gate then
    /// rides the boundary-lane clause alone (today's posture). Additive:
    /// `report_version` stays 1.
    pub workflow_semantic: Option<String>,
}

impl CheckReport {
    /// Whether the workflow is clean — conformant, no leaks, no
    /// egresses, no capability escapes, no exec-floor refusals, no
    /// policy or consent refusals, no schema-type findings, no unknown
    /// tools, no schema-lint defects. (Cost-ceiling unknowns and hints
    /// are informational, not failures.)
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.conformance.is_empty()
            && self.secret_leaks.is_empty()
            && self.secret_egresses.is_empty()
            && self.capability_escapes.is_empty()
            && self.exec_floor_findings.is_empty()
            && self.permit_taints.is_empty()
            && self.sink_findings.is_empty()
            && self.consent_findings.is_empty()
            && self.order_findings.is_empty()
            && self.lift_findings.is_empty()
            && self.trifecta_findings.is_empty()
            && self.schema_findings.is_empty()
            && self.unknown_tools.is_empty()
            && self.unknown_args.is_empty()
            && self.missing_args.is_empty()
            && self.schema_lints.is_empty()
            && self.gate_findings.is_empty()
            && self.run_decl_findings.is_empty()
            && self.write_conflicts.is_empty()
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
    /// The extra-conformance codes the Deep tier adds — every check-only
    /// invalidating surface, projected as the spec codes the conformance
    /// runner matches on. The walk lives in the private
    /// `conformance_codes` module: it is one long list of one-line arms,
    /// and it crossed the 100-line ratchet the day the unconditional
    /// order law joined it.
    #[must_use]
    pub fn extra_conformance_codes(&self) -> Vec<SpecCode> {
        conformance_codes::of(self)
    }
}

/// ONE VOICE (spec 04 §Static binding validation · conformance
/// `runner-protocol.md` class B). Since the 2026-07-30 lock the coded
/// `NIKA-VAR-003` walk in [`analyze`] owns the strict-binding law, and it
/// carries the did-you-mean the check-side rung used to be the only holder
/// of. That rung stays for the shapes the walk declares opaque, but it must
/// not repeat a reference the walk already refused: one defect printed twice
/// reads as two defects, and the conformance harness would then see a coded
/// refusal beside a codeless twin.
///
/// The match is on the BACKTICKED reference, not a bare substring — the
/// violation renders the path inside backticks, so `tasks.a.output.x` cannot
/// silently swallow a finding about `tasks.a.output.x_2`.
fn drop_refs_the_coded_walk_refused(
    findings: Vec<SchemaTypeFinding>,
    conformance: &[ConformanceViolation],
) -> Vec<SchemaTypeFinding> {
    findings
        .into_iter()
        .filter(|f| {
            let quoted = format!("`{}`", f.reference);
            !conformance
                .iter()
                .any(|v| v.code == "NIKA-VAR-003" && v.message.contains(&quoted))
        })
        .collect()
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
/// F-O8 « absent = zero authority »: a MISSING `permits:` block whose
/// body escapes NOTHING (pure compute) is the LEGAL zero — stated, not
/// punished: the hint teaches the explicit `permits: {}` form (the
/// only legal spelling of « I touch nothing »).
///
/// `judged` is false when core conformance failed. The hint asserts a property
/// OF THE BODY (« pure compute so nothing escapes »), and a body that does not
/// conform was never analysed, so the sentence would be unearned. Measured
/// 2026-08-15: a jq program reaching for the ambient environment reported its
/// `NIKA-VAR-005` refusal and this hint in the same output, one line apart.
/// Same lesson as `section_or_skip` in the renderer — a green that means
/// nobody looked is worse than no line at all.
fn legal_zero_hint(wf: &RawWorkflow, escapes_empty: bool, judged: bool, hints: &mut Vec<Hint>) {
    if judged && wf.permits.is_none() && escapes_empty {
        hints.push(Hint {
            kind: "permits",
            task: "-".to_owned(),
            advice: "no `permits:` block declared · zero authority (F-O8) — the body is \
                     pure compute so nothing escapes; declare `permits: {}` to state the \
                     zero explicitly"
                .to_owned(),
        });
    }
}

/// Map one analyzer error to its report-row form (the canonical spec
/// code · the docs URL · the did-you-mean pair) — extracted from
/// [`check`] at the fn-length ratchet.
fn conformance_violation(e: &nika_schema::error::SchemaError) -> ConformanceViolation {
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
}

#[must_use]
/// The DAG-gated finding lanes — policy reads graph ancestors, the
/// trifecta lane (NEP-0002) and the affirmative-consent lane (NEP-0020
/// · NIKA-SEC-014) share the same gating: a valid wave order or no
/// claim (skipped, never wrong). The consent scan's PROVEN
/// non-affirmative route refuses (the finding); the undecidable
/// remainder keeps the advisory hint.
fn gated_scans(
    wf: &RawWorkflow,
    conformance_clean: bool,
    edges: &[analyzer::Edge],
    topo_waves: &[Vec<usize>],
) -> (
    Vec<nika_cap::TrifectaViolation>,
    consent::ConsentScan,
    Vec<OrderFinding>,
) {
    if !conformance_clean {
        return (Vec::new(), consent::ConsentScan::default(), Vec::new());
    }
    (
        trifecta::scan_trifecta(wf, edges, topo_waves),
        consent::scan_consent(wf, edges),
        order::scan_order(wf, edges),
    )
}

pub fn check(wf: &RawWorkflow) -> CheckReport {
    let (conformance, topo_waves, edges) = match analyzer::analyze(wf) {
        Ok(AnalyzedWorkflow {
            topo_waves, edges, ..
        }) => (Vec::new(), topo_waves, edges),
        Err(errors) => (
            errors.iter().map(conformance_violation).collect(),
            Vec::new(),
            Vec::new(),
        ),
    };
    // One IFC pass over the DAG — the taint fact base both leak reports
    // read. An empty wave order (conformance failure) taints nothing:
    // the analysis is simply skipped, never wrong.
    let flow = flow::analyze_flow(wf, &topo_waves);
    // The engineering read shares the same gating: a valid order or no
    // claim (its write-write conflicts ride the dedicated finding class
    // — F-P15 · the law, never advisory).
    let dag_read = if conformance.is_empty() {
        analysis::read_dag(wf, &topo_waves)
    } else {
        analysis::DagRead::skipped()
    };
    let mut hints = hints::scan_hints(wf);
    hints.extend(native_first::scan(wf));
    // H6 · the width-capped DAG read STATES its miss (the
    // verdict-coverage law: a law that did not judge says so, in the
    // report's own surface — the JSON `hints[]` and the console HINTS
    // section both carry it).
    if let Some(miss) = dag_read.stated_miss {
        hints.push(Hint {
            kind: "analysis",
            task: "-".to_owned(),
            advice: miss,
        });
    }
    // Named once: two readers now ask « did the body conform? » — the gated
    // scans, and the legal-zero hint. Both must answer from the same fact,
    // and a predicate spelled twice is a place for them to drift apart.
    let conforms = conformance.is_empty();
    let (trifecta_findings, mut consent_scan, order_findings) =
        gated_scans(wf, conforms, &edges, &topo_waves);
    hints.extend(std::mem::take(&mut consent_scan.hints));
    let capability_escapes = permits_fit::scan_escapes(wf);
    legal_zero_hint(wf, capability_escapes.is_empty(), conforms, &mut hints);
    let cost = cost::ceiling(wf);
    let (schema_findings, unverifiable_output_refs) = schema_typing::scan_types(wf);
    let schema_findings = drop_refs_the_coded_walk_refused(schema_findings, &conformance);
    let mut report = CheckReport {
        report_version: REPORT_VERSION,
        conformance,
        energy: energy::reading(&cost),
        cost,
        certificate: certificate::certify(wf),
        requirements: requirements::collect(wf),
        permits: effective::collect(wf),
        data_journey: data_journey::collect(wf, &flow),
        secret_leaks: secrets::scan_leaks(wf, &flow),
        secret_egresses: secrets::scan_egresses(&flow),
        capability_escapes,
        // #605 · the argv exec floor as a FINDING (NIKA-SEC-001) — the
        // same predicate the run judges with (nika-types::exec), so a
        // refused-at-spawn argv refuses here first (the P0-13 hint
        // mirror is retired: one predicate, never a duplicated table).
        exec_floor_findings: exec_floor::scan(wf),
        permit_taints: permit_taint::scan_permit_taint(wf),
        sink_findings: data_sink::scan_data_sink(wf),
        consent_findings: consent_scan.findings,
        order_findings,
        lift_findings: lift::scan_idle_doors(wf),
        trifecta_findings,
        schema_findings,
        unverifiable_output_refs,
        unknown_tools: tools::scan_unknown_tools(wf),
        unknown_args: tools::scan_unknown_args(wf),
        missing_args: tools::scan_missing_args(wf),
        schema_lints: schema_lint::scan_schemas(wf),
        // gate reachability shares the IFC gating: a valid wave order or
        // nothing (skipped, never wrong)
        gate_findings: reach::scan_gates(wf, &topo_waves, &edges),
        // F-P3 · the run: declaration's body-level law (entropy: none ×
        // a structural entropy source used)
        run_decl_findings: run_decl::scan_run_decl(wf),
        // F-P15 · the write-write law (NEP-0014 law 1 · NIKA-SEC-012):
        // the DAG read's conflicts, gated on a valid order like it
        write_conflicts: dag_read.conflicts,
        // the PURE composition half (spec 14 law 1's textual part);
        // the resolved half needs a reader — `check_composed`
        composition: composition::scan_static(wf),
        hints,
        waves: topo_waves,
        analysis: dag_read.analysis,
        findings: Vec::new(),
        workflow_semantic: None,
    };
    // The class-erased list folds the FINISHED report (one truth, read
    // back) — every consumer (CLI --json · MCP nika_check) gets it free.
    report.findings = findings::collect(&report);
    report
}

/// The workflow with a CLI `--model` swapped into the envelope default
/// (#342) — per-task `model:` keeps winning, mirroring the runtime's
/// precedence. The synthetic span is fine: the pricing surfaces never
/// render the envelope model's span. The ONE home for the swap (the
/// CLI's budget preflight AND the runtime's admission gate both price
/// the EFFECTIVE model — two surfaces, one constructor, no drift).
#[must_use]
pub fn with_model_override(wf: &RawWorkflow, model: &str) -> RawWorkflow {
    let mut wf = wf.clone();
    let span = wf
        .model
        .as_ref()
        .map_or_else(nika_schema::Span::default, |m| m.span);
    wf.model = Some(nika_schema::Spanned::new(model.to_owned(), span));
    wf
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
    // The composition COST half (spec 14 · the 2026-07-29 finding): every
    // resolvable child's ceiling folds into the parent's envelope — a
    // parent whose child alone explains `≤$X` stops printing `$0 model
    // spend`. The reader-less `check` stays child-blind BY DESIGN (it
    // never loads files; the pure half needs no filesystem).
    for call in composition::price_resolved(wf, root, read) {
        report.cost.fold_composed(
            call.task,
            call.target,
            &call.child,
            call.iterations,
            call.attempts,
            call.gated,
        );
    }
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
    use nika_schema::error::SpecCategory;
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
nika: t
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
                .starts_with("https://nika.sh/language/errors/NIKA-"),
        );
    }

    #[test]
    fn clean_minimal_workflow() {
        let r = check_yaml(
            "\
nika: clean
permits: { exec: [\"echo\"] }
tasks:
  a:
    exec: { command: [\"echo\", \"hi\"] }
",
        );
        assert!(r.is_clean());
        assert_eq!(r.waves, vec![vec![0]]);
    }

    /// F-O8 « absent = zero authority » — the ERROR half: no `permits:`
    /// block + an effect ⇒ the report is dirty, every escape is stamped
    /// `undeclared`, and the code maps to NIKA-AUTH-006 on BOTH surfaces
    /// (the extra-conformance list AND the unified findings) — the
    /// refusal is proven, not supposed (EPERM-style).
    #[test]
    fn absent_permits_with_effects_is_auth_006() {
        let r = check_yaml("nika: w\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n");
        assert!(!r.is_clean(), "absent + an effect is dirty (F-O8)");
        assert!(
            r.capability_escapes
                .iter()
                .all(|e| e.undeclared && !e.floor),
            "the zero-boundary class: {:?}",
            r.capability_escapes
        );
        let codes: Vec<String> = r
            .extra_conformance_codes()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            codes.iter().any(|c| c == "NIKA-AUTH-006"),
            "the AUTH-006 code maps: {codes:?}"
        );
        assert!(
            r.findings
                .iter()
                .any(|f| f.code.as_deref() == Some("NIKA-AUTH-006")),
            "the unified finding stamps the code: {:?}",
            r.findings
        );
        // …and NO advisory hint double-teaches (the error owns the repair).
        assert!(
            !r.hints.iter().any(|h| h.kind == "permits"),
            "{:?}",
            r.hints
        );
    }

    /// F-O8 « absent = zero authority » — the LEGAL-zero half: no
    /// `permits:` block + a pure-compute body ⇒ clean, with ONE advisory
    /// hint teaching the explicit `permits: {}` form.
    #[test]
    fn pure_compute_absent_permits_gets_the_legal_zero_hint() {
        let r = check_yaml(
            "nika: w\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
        );
        assert!(r.is_clean(), "pure compute stays clean: {r:?}");
        let h = r
            .hints
            .iter()
            .find(|h| h.kind == "permits")
            .expect("the legal-zero hint rides");
        assert!(h.advice.contains("permits: {}"), "{h:?}");
        // …and `permits: {}` EXPLICIT is silent (the declared zero is
        // assumed, nothing to teach).
        let declared = check_yaml(
            "nika: w\nmodel: mock/echo\npermits: {}\ntasks:\n  a:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
        );
        assert!(declared.is_clean(), "{declared:?}");
        assert!(
            !declared.hints.iter().any(|h| h.kind == "permits"),
            "{:?}",
            declared.hints
        );
    }

    /// …and the hint is SILENT when conformance failed — it asserts a property
    /// of a body nobody analysed.
    ///
    /// Measured 2026-08-15 on the shipped 0.108.0 binary: a jq program reaching
    /// for the ambient environment printed its refusal and « the body is pure
    /// compute so nothing escapes » one line apart. Same class as the
    /// 2026-07-29 finding the renderer's `section_or_skip` closed for four
    /// other lanes — a green that means nobody looked.
    #[test]
    fn the_legal_zero_hint_is_silent_while_conformance_fails() {
        let r = check_yaml(
            "nika: w\ntasks:\n  a:\n    invoke:\n      tool: \"nika:jq\"\n      args:\n        input: {}\n        expression: 'env.PATH'\n",
        );
        assert!(
            !r.conformance.is_empty(),
            "the withheld native must be a conformance finding: {r:?}"
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "permits"),
            "the panel claimed « pure compute so nothing escapes » about a body \
             it refused one line earlier: {:?}",
            r.hints
        );
    }

    #[test]
    fn repair_loop_converges_to_clean() {
        // The agent-loop contract, AUTOMATED: a 6-finding workflow's
        // emitted fixes/suggestions, applied verbatim, reach is_clean().
        // Round 1 — assert the exact repairs the report prescribes.
        let broken = r#"nika: agent-demo
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
        // The misspelled output key repairs from the CODED voice since
        // the 2026-07-30 lock: `analyze()`'s NIKA-VAR-003 walk owns the
        // strict-binding law and carries the suggestion, and the
        // check-side rung no longer repeats it (one voice). What the
        // repair loop needs is unchanged — a did-you-mean it can apply.
        assert!(
            r.conformance
                .iter()
                .any(|v| v.code == "NIKA-VAR-003" && v.message.contains("did you mean `summary`")),
            "{:?}",
            r.conformance
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
nika: cyclic
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
        let src = "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    after: { ghost: success }\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n  b:\n    infer:\n      prompt: \"x\"\n      schema:\n        type: object\n        properties:\n          s: { type: string }\n        required: [z]\n";
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
        let yaml = "nika: w\nmodel: anthropic/claude-sonnet-4-6\npermits: { exec: false, tools: [\"nika:read\"] }\nsecrets:\n  k: { source: vault, key: x }\ntasks:\n  a:\n    invoke: { tool: \"nika:raed\", args: { path: \"./in\" } }\n  b:\n    after: { a: success }\n    exec: { command: [\"curl\", \"-d\", \"${{ secrets.k }}\", \"x\"] }\n  c:\n    with: { b_out: \"${{ tasks.b.output }}\" }\n    infer: { prompt: \"go ${{ with.b_out }}\", max_tokens: 50 }\n";
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
nika: clean
permits: { exec: [\"echo\"] }
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
    fn extra_conformance_codes_maps_bound_interpolation_to_auth_007() {
        // NEP-0004 law 1 — an interpolated permit bound is a check-time
        // security refusal, stamped NIKA-AUTH-007 on BOTH surfaces (the
        // extra-conformance list AND the unified findings) and failing
        // `is_clean` (the permit_taints arm kills `-> vec![]`).
        let r = check_yaml(
            "nika: w\npermits:\n  net: { http: [\"${{ inputs.host }}\"] }\n  tools: [\"nika:fetch\"]\ninputs:\n  host: { type: string, default: \"api.example.com\" }\ntasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/x\" }\n",
        );
        assert!(!r.is_clean(), "an interpolated bound is dirty (law 1)");
        assert_eq!(r.permit_taints.len(), 1, "{:?}", r.permit_taints);
        let codes: Vec<String> = r
            .extra_conformance_codes()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            codes.iter().any(|c| c == "NIKA-AUTH-007"),
            "the AUTH-007 code maps: {codes:?}"
        );
        let hit = r
            .findings
            .iter()
            .find(|f| f.kind == "permit_taint")
            .expect("the unified finding rides");
        assert_eq!(hit.code.as_deref(), Some("NIKA-AUTH-007"));
        assert_eq!(hit.gate, "PERMITS");
        assert!(
            hit.docs_url
                .as_deref()
                .is_some_and(|u| u.ends_with("/NIKA-AUTH-007")),
            "{hit:?}"
        );
    }

    #[test]
    fn extra_conformance_codes_maps_capability_escape_to_sec_004() {
        // A body outside the declared `permits:` → NIKA-SEC-004
        // (the capability_escapes arm). This kills `-> vec![]` AND the
        // capability-escape arm removal (without it the list is empty).
        let r = check_yaml(
            "\
nika: escape
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
    fn extra_conformance_codes_maps_run_decl_to_parse_028() {
        // F-P3 · `entropy: none` contradicted by a live retry jitter →
        // the run_decl lane refuses (is_clean fails), the class-erased
        // findings carry the row, and the code map yields the dedicated
        // NIKA-PARSE-028 mint (NEP-0010 · the 87f764a pack).
        let r = check_yaml(
            "\
nika: strict
permits: { exec: [\"flaky\"] }
run: { entropy: none }
tasks:
  flaky:
    exec: { command: [\"flaky\"] }
    retry: { max_attempts: 2, backoff_ms: 1000, jitter: true }
",
        );
        assert_eq!(r.run_decl_findings.len(), 1, "{r:?}");
        assert!(!r.is_clean(), "the contradiction is a run-blocker");
        let hit = r
            .findings
            .iter()
            .find(|f| f.kind == "run_decl")
            .expect("the row lands in findings[]");
        assert_eq!(hit.gate, "RUN");
        assert_eq!(hit.code.as_deref(), Some("NIKA-PARSE-028"));
        assert_eq!(hit.task.as_deref(), Some("flaky"));
        let codes = r.extra_conformance_codes();
        let rendered: Vec<String> = codes.iter().map(ToString::to_string).collect();
        assert!(
            rendered.iter().any(|c| c == "NIKA-PARSE-028"),
            "run_decl → NIKA-PARSE-028: {rendered:?}",
        );
    }

    #[test]
    fn extra_conformance_codes_maps_unknown_tool_to_builtin_001() {
        // A typo'd `nika:` builtin → NIKA-BUILTIN-001 (the unknown_tools
        // arm). Conformant DAG so ONLY the builtin surface fires — pins
        // the unknown_tools arm in isolation.
        let r = check_yaml(
            "\
nika: typo
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
nika: many
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

    // ── F-P15 · the write-write law (NEP-0014 law 1 · NIKA-SEC-012) ────

    /// NEGATIVE — two incomparable tasks whose literal `nika:write`
    /// paths collide is a REFUSAL: `is_clean` fails, the class-erased
    /// findings carry the row with its wire code, the code map yields
    /// NIKA-SEC-012, and NO advisory hint double-teaches (the error owns
    /// the repair — the F-O8 precedent).
    #[test]
    fn write_write_overlap_without_an_edge_refuses() {
        let r = check_yaml(
            "nika: w\npermits:\n  fs: { write: [\"out/**\"] }\n  tools: [\"nika:write\"]\ntasks:\n  left:\n    invoke: { tool: \"nika:write\", args: { path: out/report.md, content: \"a\" } }\n  right:\n    invoke: { tool: \"nika:write\", args: { path: out/report.md, content: \"b\" } }\n",
        );
        assert!(!r.is_clean(), "the unordered shared write is a finding");
        assert_eq!(r.write_conflicts.len(), 1, "{r:?}");
        let hit = r
            .findings
            .iter()
            .find(|f| f.kind == "write_conflict")
            .expect("the row lands in findings[]");
        assert_eq!(hit.gate, "WRITES");
        assert_eq!(hit.code.as_deref(), Some("NIKA-SEC-012"));
        assert_eq!(hit.task.as_deref(), Some("left"));
        assert!(
            hit.docs_url
                .as_deref()
                .is_some_and(|u| u.ends_with("/NIKA-SEC-012")),
            "{hit:?}"
        );
        let rendered: Vec<String> = r
            .extra_conformance_codes()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            rendered.iter().any(|c| c == "NIKA-SEC-012"),
            "write_conflict → NIKA-SEC-012: {rendered:?}"
        );
        assert!(
            r.extra_conformance_codes()
                .iter()
                .any(|c| c.category == SpecCategory::SecurityError),
            "the security class (NEP-0014 law 1): {:?}",
            r.extra_conformance_codes()
        );
        // …and the advisory hint era is over: no `parallel-writers` hint
        // double-teaches beside the refusal.
        assert!(
            !r.hints.iter().any(|h| h.kind == "parallel-writers"),
            "{:?}",
            r.hints
        );
    }

    /// POSITIVE — the SAME shared path with an ordering edge (`after:`)
    /// discharges the law: the writes are provably sequential, the
    /// report stays clean.
    #[test]
    fn write_write_overlap_with_an_ordering_edge_passes() {
        let r = check_yaml(
            "nika: w\npermits:\n  fs: { write: [\"out/**\"] }\n  tools: [\"nika:write\"]\ntasks:\n  first:\n    invoke: { tool: \"nika:write\", args: { path: out/report.md, content: \"a\" } }\n  second:\n    after: { first: success }\n    invoke: { tool: \"nika:write\", args: { path: out/report.md, content: \"b\" } }\n",
        );
        assert!(r.is_clean(), "ordered writers are no race: {r:?}");
        assert!(r.write_conflicts.is_empty());
    }

    /// NEGATIVE — the fan flavor: a `for_each` over one constant path
    /// refuses (every iteration would overwrite the same file).
    #[test]
    fn write_write_for_each_same_path_refuses() {
        let r = check_yaml(
            "nika: w\npermits:\n  fs: { write: [\"out/**\"] }\n  tools: [\"nika:write\"]\ntasks:\n  fan:\n    for_each: { items: [1, 2, 3] }\n    invoke: { tool: \"nika:write\", args: { path: out/same.md, content: \"x\" } }\n",
        );
        assert!(!r.is_clean(), "the fan-out overwrite is a finding");
        assert_eq!(r.write_conflicts.len(), 1, "{r:?}");
        assert_eq!(r.write_conflicts[0].task, "fan");
        assert_eq!(r.write_conflicts[0].other, None);
    }
}
