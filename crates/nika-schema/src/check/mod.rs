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

mod cost;
mod flow;
mod infer_permits;
mod permits_fit;
mod schema_typing;
mod secrets;

use crate::analyzer::{self, AnalyzedWorkflow};
use crate::error::SchemaError;
use crate::raw::RawWorkflow;

pub use cost::{CostCeiling, TaskCost, UnboundedReason};
pub use flow::{FlowFacts, TaintTrace};
pub use infer_permits::InferredPermits;
pub use permits_fit::CapabilityEscape;
pub use schema_typing::SchemaTypeFinding;
pub use secrets::{SecretEgress, SecretLeak};

/// The static pre-flight report — everything `nika check` learns without
/// running anything.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CheckReport {
    /// Topological execution waves (`waves[n]` = task indices runnable
    /// once wave `n-1` completed). The plan.
    pub waves: Vec<Vec<usize>>,
    /// Worst-case cost ceiling across all `infer:`/`agent:` tasks.
    pub cost: CostCeiling,
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
}

impl CheckReport {
    /// Whether the workflow is clean — no leaks, no egresses, no capability
    /// escapes, no schema-type findings. (Cost-ceiling unknowns are
    /// informational, not failures.)
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.secret_leaks.is_empty()
            && self.secret_egresses.is_empty()
            && self.capability_escapes.is_empty()
            && self.schema_findings.is_empty()
    }
}

/// Run the full static pre-flight over a parsed workflow.
///
/// Core conformance runs first: a workflow that does not analyze cannot
/// be pre-flighted (returns its rule violations). On success, every
/// static report is computed and collected into a [`CheckReport`].
///
/// # Errors
///
/// Returns the Core-conformance violations ([`analyze`](crate::analyze))
/// when the workflow does not pass them.
pub fn check(wf: &RawWorkflow) -> Result<CheckReport, Vec<SchemaError>> {
    let AnalyzedWorkflow { topo_waves } = analyzer::analyze(wf)?;
    // One IFC pass over the DAG — the taint fact base both leak reports read.
    let flow = flow::analyze_flow(wf, &topo_waves);
    Ok(CheckReport {
        cost: cost::ceiling(wf),
        secret_leaks: secrets::scan_leaks(wf, &flow),
        secret_egresses: secrets::scan_egresses(&flow),
        capability_escapes: permits_fit::scan_escapes(wf),
        schema_findings: schema_typing::scan_types(wf),
        waves: topo_waves,
    })
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
        check(&wf).expect("analyze")
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
    fn check_fails_on_core_violation() {
        // a cycle is a Core violation → check returns the errors, no report
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
        assert!(check(&wf).is_err(), "a cycle blocks the pre-flight");
    }
}
