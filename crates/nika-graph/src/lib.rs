// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The canonical graph projection — `graph_format: 2` (spec 03
//! §graph-projection · W2 « the flow » · typed edges). ONE projector,
//! every consumer: the CLI's `inspect --format json|mermaid|dot`, the
//! LSP's `nika/semanticDocument`, and any future surface read THIS
//! document. The edges are DERIVED from the Graph IR
//! (`nika_check::analyzer::edges` — the one computation the checker
//! and the runtime gate also consume). Format 1 is dead (no producer ·
//! no consumer · no fallback); within format 2, evolution is additive
//! and spec-first (a new field lands in 03-dag first, then here).
//!
//! The source enums are `#[non_exhaustive]` upstream, so every match
//! carries a fail-loud wildcard: a future verb / gate form / error
//! action panics HERE at projection time instead of silently emitting
//! a wrong document — enum and projector ship together.
//!
//! Moved from `nika-cli/src/verbs/graph.rs` (2026-07-13 · the semantic
//! oracle arc): the projection is language truth, not CLI presentation
//! — the renderers (mermaid · dot · ascii) stay CLI-side.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use serde::Serialize;

use nika_check::CheckReport;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::{OnErrorAction, WhenGate};

/// The versioned projection envelope (spec 03 §graph-projection · `graph_format: 2`).
#[derive(Debug, Serialize)]
pub struct GraphDoc {
    /// Envelope version — additive evolution only within 2.
    pub graph_format: u32,
    /// Workflow id (kebab-case).
    pub workflow: String,
    /// Topologically sorted nodes (wave order · stable).
    pub nodes: Vec<Node>,
    /// Typed edges (spec 03 · the CLOSED six-kind enum: value ·
    /// terminal-observation · failure-observation · control · recovery ·
    /// finally [reserved · no emission until cleanup units gain runtime
    /// identity]).
    pub edges: Vec<Edge>,
}

/// One task node — static facts only.
#[derive(Debug, Serialize)]
pub struct Node {
    /// Task id.
    pub id: String,
    /// The verb (`infer` · `exec` · `invoke` · `agent`).
    pub verb: &'static str,
    /// `invoke:` tool id, when the verb is invoke.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Resolved `<provider>/<model>` (task override → workflow default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The `when:` business condition (`"true"`/`"false"` literal or
    /// the CEL island) — POST-gate, never the gate itself (spec 03).
    pub when: Option<String>,
    /// `for_each` fan-out, when present.
    pub fan_out: Option<FanOut>,
    /// Per-task capability attribution (`exec:` · `fs.read:` ·
    /// `fs.write:` · `net.http:` · `tool:` — deterministic, BTree-ordered
    /// within each family). The un-aggregated voice of the same effect
    /// walk `infer_permits` folds into the workflow boundary; empty for
    /// a task with no pinnable effect (bare infer · dynamic net/fs).
    pub permits: Vec<String>,
    /// Static cost interval `[min_path, worst_case]` USD — only for
    /// priced inference tasks (the cost lane's honesty: no price, no
    /// interval).
    pub cost_interval: Option<[f64; 2]>,
    /// `retry.max_attempts`, when a retry policy is declared (spec 05).
    /// One voice: clients read the DECLARED policy here instead of
    /// re-parsing the YAML.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_max_attempts: Option<u32>,
    /// `timeout:` as milliseconds, when declared (spec 03 · the parsed
    /// duration — unambiguous where the source string form is not).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// `on_error:` action — `recover` · `skip` · `fail_workflow` (spec 05).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_error: Option<&'static str>,
    /// Declared `output:` binding names, in source order (spec 04) — what
    /// this task PRODUCES for `${{ tasks.<id>.<name> }}` readers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
}

/// `for_each` fan-out description.
#[derive(Debug, Serialize)]
pub struct FanOut {
    /// `"list"` (literal) or `"expression"` (source computed at run).
    pub kind: &'static str,
    /// Element count when statically known — a literal list, or an
    /// expression that is a bare immutable-authority ref over a
    /// declared literal array (`nika_check::static_literal_of` · the
    /// same count the cost lane bounds with).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

/// One typed edge (spec 03 §graph-projection · `graph_format: 2`).
#[derive(Debug, Serialize)]
pub struct Edge {
    /// Producer task id.
    pub from: String,
    /// Consumer task id.
    pub to: String,
    /// The CLOSED kind enum — `value` · `terminal-observation` ·
    /// `failure-observation` · `control` · `recovery` (· `finally`
    /// reserved).
    pub kind: &'static str,
    /// The `after:` predicate (`success` · `failure` · `skipped` ·
    /// `terminal`) — control edges only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicate: Option<&'static str>,
    /// The `with:` key that created a data/observation edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
}

/// Project a checked workflow into the canonical graph document.
///
/// Requires a clean-conformance report: without a valid DAG order there
/// is no topological node list to project.
#[must_use]
pub fn project(wf: &RawWorkflow, report: &CheckReport) -> GraphDoc {
    let workflow = wf
        .workflow
        .as_ref()
        .map_or_else(|| "workflow".to_owned(), |w| w.value.clone());
    let default_model = wf.model.as_ref().map(|m| m.value.clone());

    let mut nodes = Vec::with_capacity(wf.tasks.len());
    for wave in &report.waves {
        for &index in wave {
            let task = &wf.tasks[index].value;
            let (verb, tool, model) = action_facts(&task.action, default_model.as_deref());
            let cost_interval = report
                .cost
                .tasks
                .iter()
                .find(|c| c.task == task.id.value)
                .and_then(|c| c.min_path_usd.zip(c.usd).map(|(min, max)| [min, max]));
            nodes.push(Node {
                id: task.id.value.clone(),
                verb,
                tool,
                model,
                when: task.when.as_ref().map(|w| match &w.value {
                    // CLOSED vocabulary (nika-vocab) — a new gate form is
                    // a spec change that must land HERE explicitly.
                    WhenGate::Literal(b) => b.to_string(),
                    WhenGate::Expr(e) => e.clone(),
                }),
                permits: nika_check::task_permits(task),
                fan_out: task.for_each.as_ref().map(|f| match &f.value {
                    nika_schema::raw::ForEachValue::List(items) => FanOut {
                        kind: "list",
                        count: items.as_array().map(|a| a.len() as u64),
                    },
                    // A bare immutable-authority ref over a declared
                    // literal array has a statically-known count — the
                    // ONE shared resolver, the same count the cost lane
                    // bounds with (probe 2026-07-30: the inspect header
                    // said « no task runs » while this node row said
                    // `for_each ×?` — one surface, two voices).
                    nika_schema::raw::ForEachValue::Expression(expr) => FanOut {
                        kind: "expression",
                        count: nika_check::static_literal_of(wf, expr)
                            .and_then(|v| v.as_array())
                            .map(|a| a.len() as u64),
                    },
                    #[allow(
                        clippy::unreachable,
                        reason = "non_exhaustive future variant — enum and projector ship together; fail loud beats silently-wrong output"
                    )]
                    other => {
                        unreachable!("unknown for_each form: {other:?}")
                    }
                }),
                cost_interval,
                retry_max_attempts: task.retry.as_ref().map(|r| r.value.max_attempts),
                timeout_ms: task
                    .timeout
                    .as_ref()
                    .map(|t| u64::try_from(t.value.as_millis()).unwrap_or(u64::MAX)),
                on_error: task.on_error.as_ref().map(|o| match &o.value.action {
                    OnErrorAction::Recover(_) => "recover",
                    OnErrorAction::Skip => "skip",
                    OnErrorAction::FailWorkflow => "fail_workflow",
                    #[allow(
                        clippy::unreachable,
                        reason = "non_exhaustive future variant — enum and projector ship together; fail loud beats silently-wrong output"
                    )]
                    other => unreachable!("unknown on_error action: {other:?}"),
                }),
                outputs: task.output.iter().map(|(k, _)| k.value.clone()).collect(),
            });
        }
    }

    GraphDoc {
        graph_format: 2,
        workflow,
        nodes,
        edges: typed_edges(wf),
    }
}

/// The typed edges — the SAME derivation the checker and the runtime
/// gate consume (one-truth · spec 03 §the four graphs), plus the
/// recovery reads (`E_r` · parking · never scheduling · spec 05).
/// Stable order (from · to · kind · binding · predicate) · exact
/// duplicates collapse (a twice-written identical reference adds
/// nothing the gate reads).
fn typed_edges(wf: &RawWorkflow) -> Vec<Edge> {
    let ids: std::collections::BTreeMap<String, usize> = wf
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.value.id.value.clone(), i))
        .collect();
    let id_of = |ix: usize| wf.tasks[ix].value.id.value.clone();
    let mut edges: Vec<Edge> = nika_check::analyzer::edges::derive_edges(&wf.tasks, &ids)
        .into_iter()
        .map(|e| Edge {
            from: id_of(e.from),
            to: id_of(e.to),
            kind: e.kind.wire_kind(),
            predicate: match e.kind {
                nika_check::analyzer::EdgeKind::Control(p) => Some(p.as_str()),
                _ => None,
            },
            binding: e.binding,
        })
        .collect();
    edges.extend(
        nika_check::analyzer::edges::derive_recovery_reads(&wf.tasks, &ids)
            .into_iter()
            .map(|r| Edge {
                from: id_of(r.from),
                to: id_of(r.to),
                kind: "recovery",
                predicate: None,
                binding: None,
            }),
    );
    edges.sort_by(|a, b| {
        (&a.from, &a.to, a.kind, &a.binding, a.predicate).cmp(&(
            &b.from,
            &b.to,
            b.kind,
            &b.binding,
            b.predicate,
        ))
    });
    edges.dedup_by(|a, b| {
        a.from == b.from
            && a.to == b.to
            && a.kind == b.kind
            && a.binding == b.binding
            && a.predicate == b.predicate
    });

    edges
}

/// (verb, tool, resolved model) for one action.
fn action_facts(
    action: &RawAction,
    default_model: Option<&str>,
) -> (&'static str, Option<String>, Option<String>) {
    match action {
        RawAction::Infer(infer) => (
            "infer",
            None,
            infer
                .model
                .as_ref()
                .map(|m| m.value.clone())
                .or_else(|| default_model.map(str::to_owned)),
        ),
        RawAction::Exec(_) => ("exec", None, None),
        RawAction::Invoke(invoke) => match &invoke.target {
            nika_schema::raw::RawInvokeTarget::Tool(t) => ("invoke", Some(t.value.clone()), None),
            nika_schema::raw::RawInvokeTarget::Workflow(w) => {
                ("invoke", Some(format!("workflow:{}", w.value)), None)
            }
        },
        RawAction::Agent(agent) => (
            "agent",
            None,
            agent
                .model
                .as_ref()
                .map(|m| m.value.clone())
                .or_else(|| default_model.map(str::to_owned)),
        ),
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future verb — it must teach this projector its name before it can be drawn"
        )]
        other => unreachable!("unknown verb: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One flattened edge row (from · to · kind · predicate · binding).
    type Row<'a> = (&'a str, &'a str, &'a str, Option<&'a str>, Option<&'a str>);
    use nika_schema::{FileId, ParseMode, parse};

    fn doc(yaml: &str) -> GraphDoc {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "fixture must be clean");
        project(&wf, &report)
    }

    /// Nodes come out WAVE-ORDERED (Kahn levels · stable layouts): in
    /// the diamond a → {b, c} → d authored out of order, the projection
    /// still reads a · b · c · d.
    #[test]
    fn nodes_are_wave_ordered_regardless_of_authoring_order() {
        let g = doc(
            "nika: v1\nworkflow:\n  id: w\npermits: { exec: [\"true\"] }\ntasks:\n  d:\n    with:\n      b: \"${{ tasks.b.output }}\"\n      c: \"${{ tasks.c.output }}\"\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n  a:\n    exec: { command: [\"true\"] }\n  c:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n",
        );
        assert_eq!(g.graph_format, 2);
        let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids[0], "a", "wave 1 first: {ids:?}");
        assert_eq!(ids[3], "d", "the join lands last: {ids:?}");
    }

    /// Edges carry their ROLE: a value binding · a control predicate ·
    /// a recovery read — sorted, exact duplicates collapsed.
    #[test]
    fn edges_carry_kind_predicate_and_binding() {
        let g = doc(
            "nika: v1\nworkflow:\n  id: w\npermits: { exec: [\"true\"] }\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: terminal }\n    with:\n      st: \"${{ tasks.a.status }}\"\n    exec: { command: [\"true\"] }\n  c:\n    exec: { command: [\"true\"] }\n    on_error:\n      recover: \"${{ tasks.a.output }}\"\n",
        );
        let rows: Vec<Row<'_>> = g
            .edges
            .iter()
            .map(|e| {
                (
                    e.from.as_str(),
                    e.to.as_str(),
                    e.kind,
                    e.predicate,
                    e.binding.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("a", "b", "control", Some("terminal"), None),
                ("a", "b", "terminal-observation", None, Some("st")),
                ("a", "c", "recovery", None, None),
            ],
            "typed · sorted · the roles compose: {rows:?}"
        );
    }

    /// A twice-written identical reference collapses — the wire never
    /// lies about cardinality (same law the W1 dedup had).
    #[test]
    fn edges_are_sorted_and_deduped() {
        let g = doc(
            "nika: v1\nworkflow:\n  id: w\npermits: { exec: [\"true\"] }\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    with:\n      x: \"${{ tasks.a.output }} and ${{ tasks.a.output }}\"\n    exec: { command: [\"true\"] }\n",
        );
        let pairs: Vec<(&str, &str)> = g
            .edges
            .iter()
            .map(|e| (e.from.as_str(), e.to.as_str()))
            .collect();
        assert_eq!(pairs, vec![("a", "b")], "deduped: {pairs:?}");
        assert!(g.edges.iter().all(|e| e.kind == "value"));
    }

    /// The static facts ride the node: verb · resolved model (task
    /// override beats the workflow default) · declared output names.
    #[test]
    fn node_facts_are_projected() {
        let g = doc(
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\ntasks:\n  think:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n    output:\n      summary: \".\"\n",
        );
        let n = &g.nodes[0];
        assert_eq!(n.verb, "infer");
        assert_eq!(n.model.as_deref(), Some("mock/echo"), "workflow default");
        assert_eq!(n.outputs, vec!["summary"], "declared bindings in order");
    }
}
