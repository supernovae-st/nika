// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Child-workflow execution — the composition seam (spec `14-composition.md`).
//!
//! A task `invoke: { workflow: <static target>, args: {…} }` executes the
//! child as a NESTED RUN. The runtime owns the laws it can judge locally
//! (the `NIKA-SEC-003` depth backstop · budget/deadline inheritance inputs ·
//! the trace-forest record); the CALLER owns the I/O half through this
//! injected seam (resolve the target, compose a child runtime, run it) —
//! the same shape as every other kernel edge: the L3 runtime stays
//! filesystem-free.
//!
//! The trace forest (law 8): the child keeps its OWN hash-chain; the
//! parent's terminal frame records the child's `{trace_id, chain_head,
//! def_hash, outcome}`. Because every parent frame is itself hash-chained,
//! embedding the child's chain head makes the parent's receipt COMMIT to
//! the child's (law 9 · Merkle composition — a proof of the whole contains
//! a proof of each part).

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use nika_schema::types::Permits;
use serde_json::Value;

/// The run-recursion bound (`NIKA-SEC-003` · spec `14-composition.md`
/// §errors). Defense in depth: the PRIMARY guard is static acyclicity
/// (`NIKA-COMP-003` at check); this cap refuses fail-closed at run the
/// cases a static checker cannot draw (an embedder that skipped check ·
/// a registry child resolving deeper than its pin promised).
pub const MAX_RUN_DEPTH: u32 = 8;

/// One child-workflow call — everything the parent knows at the call
/// site, handed to the injected [`ChildRunner`].
#[derive(Debug, Clone)]
pub struct ChildCall {
    /// The static target as written (`./child.nika.yaml` ·
    /// `registry:owner/name@version`) — check proved it resolvable
    /// (`NIKA-COMP-001`); the runner resolves it relative to the PARENT
    /// workflow's own location.
    pub target: String,
    /// Rendered call args — the child's `vars:` inputs (spec 14 law 2 ·
    /// the typed call; the child's own `--var` refusal law applies).
    pub args: BTreeMap<String, Value>,
    /// The CHILD's nesting depth (root run = 0 · its children = 1 · …).
    /// Already depth-gated by the runtime before the runner is called.
    pub depth: u32,
    /// Parent budget remaining at call time (USD) — the child runs under
    /// `min(parent remaining, child declared)` (law 6). `None` = the
    /// parent run carries no cost budget.
    pub remaining_budget_usd: Option<f64>,
    /// The parent task's `timeout:` budget — the attempt loop already
    /// bounds the WHOLE dispatch with it (a child cannot outlive its
    /// caller: the child future is dropped at the deadline); the runner
    /// may additionally hand it to the child for graceful bounding.
    pub deadline: Option<Duration>,
    /// The parent run's declared capability boundary — the runner MUST
    /// compose the child's effective boundary as `child ⊆ parent ∩
    /// declared` (law 3/4 · the check-time `NIKA-COMP-002` proof made
    /// structural at run).
    pub parent_permits: Option<Permits>,
}

/// What the parent records about a finished child run — the trace-forest
/// row (law 8) + the receipt commitment (law 9).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ChildRunSummary {
    /// The target as written at the call site.
    pub target: String,
    /// The child's trace identity (the runner's trace file stem / run id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// The head of the child's OWN hash-chain — embedding it in the
    /// parent's (chained) frame is the Merkle commitment of law 9.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_head: Option<String>,
    /// The child definition's content hash (the source bytes the child
    /// run was launched from). The SEMANTIC identity (canonical Semantic
    /// IR · law 10) is W6's — this is the honest pre-W6 anchor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub def_hash: Option<String>,
    /// The child run's terminal class — `success` | `failure`.
    pub outcome: String,
}

impl ChildRunSummary {
    /// Construct a summary row (INV-019 · `#[non_exhaustive]` structs
    /// ship a constructor). `ok` folds to the closed outcome vocabulary.
    #[must_use]
    pub fn new(
        target: impl Into<String>,
        ok: bool,
        (trace_id, chain_head, def_hash): (Option<String>, Option<String>, Option<String>),
    ) -> Self {
        Self {
            target: target.into(),
            trace_id,
            chain_head,
            def_hash,
            outcome: if ok { "success" } else { "failure" }.to_owned(),
        }
    }

    /// The frame value the settle pass emits under `child`.
    #[must_use]
    pub fn json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// A finished child run, as the runner reports it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ChildOutcome {
    /// Whether the child run settled green.
    pub ok: bool,
    /// The child's typed `outputs:` map — the parent task's value
    /// (spec 14 law 2 · the child's outputs fit the parent's `returns:`).
    pub outputs: BTreeMap<String, Value>,
    /// The child run's total metered spend — the parent ledger debits it
    /// (law 5/6 · resources summed · one count, attributed to the call).
    pub cost_usd: Option<f64>,
    /// The trace-forest row (absent when the runner kept no trace —
    /// e.g. a hermetic test runner).
    pub trace: Option<ChildRunSummary>,
    /// The child's failure surface when `ok == false` — `(code, message)`
    /// of the FIRST terminal failure (the child's own spec-plane code).
    pub failure: Option<(String, String)>,
    /// One deterministic selected route from the child run, when one
    /// exists. This is a replay guard, not an exhaustive list: a child
    /// with multiple effects prefers its first harness receipt in task-id
    /// order. A parent retry must preserve it on success and failure.
    pub access_receipt: Option<crate::AccessReceipt>,
}

impl ChildOutcome {
    /// Construct a child terminal result, including its execution-route
    /// receipt when the nested run selected one.
    #[must_use]
    pub fn new(
        ok: bool,
        outputs: BTreeMap<String, Value>,
        cost_usd: Option<f64>,
        trace: Option<ChildRunSummary>,
        failure: Option<(String, String)>,
        access_receipt: Option<crate::AccessReceipt>,
    ) -> Self {
        Self {
            ok,
            outputs,
            cost_usd,
            trace,
            failure,
            access_receipt,
        }
    }
}

/// A composition refusal from the runner — the run-side voice of the
/// check-time `NIKA-COMP` findings (the skills `NIKA-AGENT-003/004`
/// dual-surface precedent: check refuses first; this path fires for an
/// embedder that skipped the contract — fail the task loudly).
#[derive(Debug, Clone)]
pub struct ChildRunRefusal {
    /// The spec-plane code (`NIKA-COMP-001` unresolvable · `NIKA-COMP-002`
    /// containment · `NIKA-COMP-004` typed-call · `NIKA-SEC-003` depth).
    pub code: String,
    /// The human detail (names the exact repair).
    pub message: String,
}

/// The child-execution seam — implemented by the CALLER (the CLI's
/// production composer · a hermetic mock in tests). Dyn-compatible by
/// construction (boxed future) so the runtime can hold it type-erased:
/// a generic child runtime would recurse the `Runtime` type parameters.
///
/// The future is deliberately NOT `Send`: a nested run drives
/// [`crate::Runtime::run`], whose future holds the `&mut dyn Stamper` /
/// `&mut dyn EventSink` seams (erased without auto-trait bounds), and
/// every run executes on a current-thread executor by design (the CLI's
/// `block_on`). A multi-threaded embedder adds a `Send` lane the day it
/// exists — never speculatively.
pub trait ChildRunner: Send + Sync {
    /// Execute one child call to completion (or refusal).
    ///
    /// The runtime has ALREADY gated the depth (`MAX_RUN_DEPTH` ·
    /// fail-closed `NIKA-SEC-003`) and rendered the args; the runner
    /// resolves + parses + checks + runs the child, composing budgets
    /// (`min(remaining, declared)`), permits (`child ∩ parent`) and the
    /// child's own trace sink.
    fn run_child<'a>(
        &'a self,
        call: ChildCall,
    ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_json_carries_the_forest_row() {
        let s = ChildRunSummary::new(
            "./child.nika.yaml",
            true,
            (
                Some("run-1".to_owned()),
                Some("abc123".to_owned()),
                Some("deadbeef".to_owned()),
            ),
        );
        let v = s.json();
        assert_eq!(v["target"], "./child.nika.yaml");
        assert_eq!(v["chain_head"], "abc123");
        assert_eq!(v["outcome"], "success");
    }

    #[test]
    fn summary_json_omits_absent_fields() {
        let s = ChildRunSummary::new("t", false, (None, None, None));
        let v = s.json();
        assert!(v.get("chain_head").is_none(), "{v}");
        assert!(v.get("trace_id").is_none(), "{v}");
    }
}
