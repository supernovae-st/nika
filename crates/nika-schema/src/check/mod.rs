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
//! It composes [`analyze`](crate::analyze) (Core conformance) with four
//! static reports over the [`RawWorkflow`] ·
//!
//! - **plan** — the topological wave structure (who runs in parallel)
//! - **cost ceiling** — worst-case spend · `Σ max_tokens × provider price`
//! - **secret leaks** — `secrets.X` flowing into an `exec`/tool capture
//! - **capability escapes** — effects outside a declared `permits:` block
//!
//! `check` is read-only and never executes a verb.

mod cost;
mod permits_fit;
mod secrets;

use crate::analyzer::{self, AnalyzedWorkflow};
use crate::error::SchemaError;
use crate::raw::RawWorkflow;

pub use cost::{CostCeiling, TaskCost, UnboundedReason};
pub use permits_fit::CapabilityEscape;
pub use secrets::SecretLeak;

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
    /// Every `secrets.X` that escapes the masking boundary (flows into an
    /// `exec`/tool whose output is captured).
    pub secret_leaks: Vec<SecretLeak>,
    /// Every statically-detectable effect outside the declared `permits:`
    /// boundary (empty when no `permits:` block is present).
    pub capability_escapes: Vec<CapabilityEscape>,
}

impl CheckReport {
    /// Whether the workflow is clean — no leaks, no capability escapes.
    /// (Cost-ceiling unknowns are informational, not failures.)
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.secret_leaks.is_empty() && self.capability_escapes.is_empty()
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
    Ok(CheckReport {
        waves: topo_waves,
        cost: cost::ceiling(wf),
        secret_leaks: secrets::scan_leaks(wf),
        capability_escapes: permits_fit::scan_escapes(wf),
    })
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
