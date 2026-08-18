// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Graph IR — ONE edge derivation, every surface projects.
//!
//! Spec `03-dag.md` §the four graphs · §gate algebra v2 (W2 « the
//! flow »). A workflow's scheduling edges are DERIVED from exactly two
//! declaration surfaces — `with:` bindings (`E_d` · role per referenced
//! field) and `after:` entries (`E_c` · predicate per entry) — plus the
//! non-scheduling recovery reads (`E_r` · `on_error.recover`). The
//! analyzer checks, the runtime gate, the `graph_format: 3` projection,
//! the LSP lanes and the codemod ALL consume this module; none re-walks
//! the AST for edges (one-truth · the pre-W2 world had three parallel
//! walks and NIKA-DAG-003 existed to keep them honest).
//!
//! Pass-sets are CONTEXT-FREE: an edge's kind and predicate alone
//! determine admission (the reference model `reference/semantics.py` is
//! the oracle · differential-tested).

use std::collections::{BTreeMap, BTreeSet};

use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
use nika_schema::raw::RawTask;
use nika_schema::source::{Span, Spanned};
use nika_schema::types::{AfterPredicate, OnErrorAction};

/// A producer's settled state — the domain the gate algebra judges.
///
/// The runtime maps its richer task status onto this at the ONE seam
/// (`pending`/`running` never reach a gate: an edge is consulted only
/// once its producer is terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SettledState {
    /// The producer completed (a recovered producer settles here too).
    Success,
    /// The producer failed unrecovered.
    Failure,
    /// The producer was skipped (`when: false` · empty `for_each` ·
    /// `on_error: skip`).
    Skipped,
    /// The producer was cancelled (a dead path · workflow cancellation).
    Cancelled,
}

/// The edge role — spec `03-dag.md` §with (`E_d` roles) + §after (`E_c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EdgeKind {
    /// A `.output` / named-binding reference — pass `{success, skipped}`
    /// (the skipped value reads defined-`null`).
    Value,
    /// A `.status`/`.cause`/`.duration_ms`/`.started_at`/`.ended_at`
    /// reference — every settled state admits (OBSERVE the outcome).
    TerminalObservation,
    /// An `.error` reference — pass `{failure, skipped}` (a skip may
    /// carry a PRESERVED error · `on_error: skip` · spec 05 §Fields; a
    /// decision-skip's error reads defined-`null`).
    FailureObservation,
    /// A `${{ group.<name> }}` fold — ONE edge per declared member
    /// (spec 03 §the fan-in edge). Pass-set is all four settled states,
    /// DELIBERATELY not the intersection of its members' field roles: a
    /// value edge admits `{success, skipped}` and a failure-observation
    /// `{failure, skipped}`, so an intersection would leave `{skipped}`
    /// and every fold would be `NIKA-DAG-006`-dead on arrival. The fold
    /// is a terminal observation of each member — it runs whatever
    /// happened, and the per-member truth lives in the record.
    FanIn,
    /// An `after:` entry — pass per its predicate.
    Control(AfterPredicate),
}

impl EdgeKind {
    /// GATE-v2 admission — whether a producer settled `s` admits the
    /// consumer through THIS edge (spec `03-dag.md` §gate algebra v2 ·
    /// mirrored by `reference/semantics.py` pass-sets).
    #[must_use]
    pub fn admits(self, s: SettledState) -> bool {
        use SettledState::{Cancelled, Failure, Skipped, Success};
        match self {
            Self::Value => matches!(s, Success | Skipped),
            Self::TerminalObservation | Self::FanIn => true,
            Self::FailureObservation => matches!(s, Failure | Skipped),
            Self::Control(p) => match p {
                AfterPredicate::Success => matches!(s, Success),
                AfterPredicate::Failure => matches!(s, Failure),
                AfterPredicate::Skipped => matches!(s, Skipped),
                AfterPredicate::Terminal => {
                    matches!(s, Success | Failure | Skipped | Cancelled)
                }
                // Not a settle-state comparison · the E_f attachment
                // fires for a producer that STARTED, whatever it became.
                // It can never be the edge that proves a task dead —
                // `is_scheduling` keeps it out of that analysis entirely.
                AfterPredicate::Unwind => true,
            },
        }
    }

    /// Whether this edge belongs to `G_p`, the PRECEDENCE graph.
    ///
    /// `false` for `unwind` alone: an `E_f` attachment does not schedule,
    /// does not participate in cycle detection and does not enter wave
    /// assignment (spec 03 §the rest of the contract · « an engine that
    /// adds them to the precedence graph is wrong »). Structural rather
    /// than a filter at each call site, so a new pass cannot forget it.
    #[must_use]
    pub fn is_scheduling(self) -> bool {
        !matches!(self, Self::Control(AfterPredicate::Unwind))
    }

    /// The `graph_format: 3` wire kind.
    #[must_use]
    pub fn wire_kind(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::TerminalObservation => "terminal-observation",
            Self::FailureObservation => "failure-observation",
            Self::FanIn => "fan-in",
            // E_f is its OWN row in the four-graphs table, not a control
            // predicate that happens to be spelled `unwind`. Reading it
            // as `control` told every client the cleanup attachment
            // schedules like an ordering dependency — it does not
            // (`schedules()` has always said so; only the WIRE lied).
            Self::Control(AfterPredicate::Unwind) => "finally",
            Self::Control(_) => "control",
        }
    }
}

/// One derived scheduling edge (`from` produces · `to` consumes).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Edge {
    /// Producer task index (into the analyzed task list).
    pub from: usize,
    /// Consumer task index.
    pub to: usize,
    /// The role (and predicate for control edges).
    pub kind: EdgeKind,
    /// The `with:` key that created a data/observation edge (`None`
    /// for control edges) — the `graph_format: 3` `binding` field.
    pub binding: Option<String>,
    /// The declaration site (the binding value · the `after:` entry).
    pub span: Span,
}

/// A non-scheduling recovery read (`on_error.recover` · `E_r`) — projected
/// as a `recovery` edge in `graph_format: 3`, never gating, never
/// ordering (the parking-table semantics · `NIKA-DAG-004` guards the
/// deadlock).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecoveryRead {
    /// The read task (the fallback source).
    pub from: usize,
    /// The declaring task (whose `on_error.recover` reads it).
    pub to: usize,
}

/// The edge role a referenced result-record field carries (spec
/// `03-dag.md` §with · the role table).
#[must_use]
pub fn role_of_field(field: Option<&str>) -> EdgeKind {
    const OBSERVE: [&str; 5] = ["status", "cause", "duration_ms", "started_at", "ended_at"];
    match field {
        Some(f) if OBSERVE.contains(&f) => EdgeKind::TerminalObservation,
        Some("error") => EdgeKind::FailureObservation,
        // `.output`, a named `output:` binding, or a bare envelope
        // (the scan layer rejects the bare form as NIKA-VAR-020).
        _ => EdgeKind::Value,
    }
}

/// Every `tasks.<id>` reference (with its field) inside the `${{ }}`
/// islands of a JSON value tree — the boundary walk shared by the
/// derivation, the lints and the projections.
pub fn task_refs_in_value(value: &serde_json::Value, out: &mut Vec<(String, Option<String>)>) {
    match value {
        serde_json::Value::String(s) => {
            let Ok(islands) = scan_templates(s) else {
                return; // malformed · the scan layer reports it
            };
            for island in islands {
                for r in expr_refs(&island.expr) {
                    if let NamespaceRef::Tasks { id, field } = r {
                        out.push((id, field));
                    }
                }
            }
        }
        serde_json::Value::Array(items) => {
            for i in items {
                task_refs_in_value(i, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                task_refs_in_value(v, out);
            }
        }
        _ => {}
    }
}

/// group name → the member task indices, in DECLARATION order.
///
/// A group exists IFF at least one task declares it — there is no glob,
/// no prefix match, no regex. That is the load-bearing choice: with a
/// pattern, renaming a member silently shrinks every fold that globbed
/// it and the run stays GREEN while covering less. Declared membership
/// turns the same rename into `NIKA-DAG-008`, at check time.
#[must_use]
pub fn group_members(tasks: &[Spanned<RawTask>]) -> BTreeMap<&str, Vec<usize>> {
    let mut out: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, task) in tasks.iter().enumerate() {
        if let Some(g) = &task.value.group {
            out.entry(g.value.as_str()).or_default().push(i);
        }
    }
    out
}

/// Every `group.<name>` fold inside the `${{ }}` islands of a value tree
/// (the bare `${{ group }}` yields an empty name — `NIKA-DAG-008`).
pub fn group_refs_in_value(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => {
            let Ok(islands) = scan_templates(s) else {
                return; // malformed · the scan layer reports it
            };
            for island in islands {
                for r in expr_refs(&island.expr) {
                    if let NamespaceRef::Group(name) = r {
                        out.push(name);
                    }
                }
            }
        }
        serde_json::Value::Array(items) => {
            for i in items {
                group_refs_in_value(i, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                group_refs_in_value(v, out);
            }
        }
        _ => {}
    }
}

/// THE derivation — `E_d` from `with:` (one edge per static reference ·
/// role per field) ∪ `E_c` from `after:` (one control edge per entry).
///
/// Unresolvable targets are skipped (`NIKA-DAG-002` reports them); an
/// unknown predicate derives no edge (`NIKA-DAG-005` reports it). The
/// result is deterministic: task order · then `with:` source order ·
/// then `after:` source order.
#[must_use]
pub fn derive_edges(tasks: &[Spanned<RawTask>], ids: &BTreeMap<String, usize>) -> Vec<Edge> {
    let members = group_members(tasks);
    let mut edges = Vec::new();
    for (to, task) in tasks.iter().enumerate() {
        for (key, value) in &task.value.with {
            let mut refs = Vec::new();
            task_refs_in_value(&value.value, &mut refs);
            for (id, field) in refs {
                let Some(&from) = ids.get(&id) else {
                    continue; // DAG-002's report
                };
                edges.push(Edge {
                    from,
                    to,
                    kind: role_of_field(field.as_deref()),
                    binding: Some(key.value.clone()),
                    span: value.span,
                });
            }
            // the PLURAL of the data edge — one edge per declared member,
            // in DECLARATION order (the source order of `tasks:`), so the
            // fold's shape is stable across re-runs where completion
            // order would not be (spec 03 §the member record).
            let mut folds = Vec::new();
            group_refs_in_value(&value.value, &mut folds);
            for name in folds {
                let Some(idx) = members.get(name.as_str()) else {
                    continue; // DAG-008's report
                };
                for &from in idx {
                    edges.push(Edge {
                        from,
                        to,
                        kind: EdgeKind::FanIn,
                        binding: Some(key.value.clone()),
                        span: value.span,
                    });
                }
            }
        }
        for (target, predicate) in &task.value.after {
            let Some(&from) = ids.get(&target.value) else {
                continue; // DAG-002's report
            };
            edges.push(Edge {
                from,
                to,
                kind: EdgeKind::Control(predicate.value),
                binding: None,
                span: target.span,
            });
        }
    }
    edges
}

/// A task's incoming edges from its OWN declarations — `(producer id,
/// kind)` in declaration order (`with:` refs then `after:` entries ·
/// duplicates kept: edges compose by intersection). The runtime gate
/// consumes THIS (targets are checker-validated before any run).
#[must_use]
pub fn incoming_of(task: &RawTask) -> Vec<(String, EdgeKind)> {
    let mut out = Vec::new();
    for (_key, value) in &task.with {
        let mut refs = Vec::new();
        task_refs_in_value(&value.value, &mut refs);
        for (id, field) in refs {
            out.push((id, role_of_field(field.as_deref())));
        }
    }
    for (target, pred) in &task.after {
        out.push((target.value.clone(), EdgeKind::Control(pred.value)));
    }
    out
}

/// A task's producer ids, by declaration order (`with:` refs then
/// `after:` targets · deduped) — the certificate rows and the blast
/// analyses read THIS, never a private AST walk.
#[must_use]
pub fn producer_ids(task: &RawTask) -> Vec<String> {
    // First-seen order preserved (the certificate rows are order-stable);
    // dedup rides a set — a `Vec::contains` scan here was O(N²) on a wide
    // fan-in (10k `after:` entries ≈ 85 ms of `check` · the same class
    // reach.rs documents at Gate-11 F2).
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<String> = Vec::new();
    for (_key, value) in &task.with {
        let mut refs = Vec::new();
        task_refs_in_value(&value.value, &mut refs);
        for (id, _field) in refs {
            if seen.insert(id.clone()) {
                out.push(id);
            }
        }
    }
    for (target, _pred) in &task.after {
        if seen.insert(target.value.clone()) {
            out.push(target.value.clone());
        }
    }
    out
}

/// The recovery reads (`E_r`) — every `tasks.<id>` referenced inside a
/// task's `on_error.recover` value, resolved.
#[must_use]
pub fn derive_recovery_reads(
    tasks: &[Spanned<RawTask>],
    ids: &BTreeMap<String, usize>,
) -> Vec<RecoveryRead> {
    let mut reads = Vec::new();
    for (to, task) in tasks.iter().enumerate() {
        let Some(on_error) = &task.value.on_error else {
            continue;
        };
        let OnErrorAction::Recover(value) = &on_error.value.action else {
            continue;
        };
        let mut refs = Vec::new();
        task_refs_in_value(&value.value, &mut refs);
        refs.sort();
        refs.dedup();
        for (id, _field) in refs {
            if let Some(&from) = ids.get(&id) {
                reads.push(RecoveryRead { from, to });
            }
        }
    }
    reads
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn edges_of(yaml: &str) -> (Vec<Edge>, Vec<String>) {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let ids: BTreeMap<String, usize> = wf
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.value.id.value.clone(), i))
            .collect();
        let names = wf.tasks.iter().map(|t| t.value.id.value.clone()).collect();
        (derive_edges(&wf.tasks, &ids), names)
    }

    const HEADER: &str = "nika: t\n";

    #[test]
    fn with_binding_is_the_edge_role_per_field() {
        // 03 §with · one binding expression with N refs = N edges ·
        // role per referenced field. `.cause` (spec 13 §one obvious
        // way) is terminal-observation, the SAME pass-set as `.status`.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    exec: {{ command: [\"true\"] }}
  b:
    with:
      data: ${{{{ tasks.a.output }}}}
      took: ${{{{ tasks.a.duration_ms }}}}
      err: ${{{{ tasks.a.error }}}}
      why: ${{{{ tasks.a.cause }}}}
    exec: {{ command: [\"true\"] }}
"
        );
        let (edges, _) = edges_of(&yaml);
        assert_eq!(edges.len(), 4);
        assert_eq!(edges[0].kind, EdgeKind::Value);
        assert_eq!(edges[0].binding.as_deref(), Some("data"));
        assert_eq!(edges[1].kind, EdgeKind::TerminalObservation);
        assert_eq!(edges[2].kind, EdgeKind::FailureObservation);
        assert_eq!(edges[3].kind, EdgeKind::TerminalObservation);
        assert_eq!(edges[3].binding.as_deref(), Some("why"));
        assert!(edges.iter().all(|e| e.from == 0 && e.to == 1));
    }

    #[test]
    fn after_entry_is_a_control_edge_with_its_predicate() {
        let yaml = format!(
            "{HEADER}tasks:
  a:
    exec: {{ command: [\"true\"] }}
  b:
    after: {{ a: terminal }}
    exec: {{ command: [\"true\"] }}
"
        );
        let (edges, _) = edges_of(&yaml);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].kind, EdgeKind::Control(AfterPredicate::Terminal));
        assert_eq!(edges[0].binding, None);
    }

    #[test]
    fn gate_algebra_pass_sets_match_the_reference_model() {
        // reference/semantics.py · PASS_VALUE {success, skipped} ·
        // PASS_TERMINAL_OBS all four · PASS_FAILURE_OBS {failure,
        // skipped} · control per predicate (terminal incl. cancelled).
        use SettledState::{Cancelled, Failure, Skipped, Success};
        let all = [Success, Failure, Skipped, Cancelled];
        let admit = |k: EdgeKind| all.iter().filter(|&&s| k.admits(s)).count();
        assert_eq!(admit(EdgeKind::Value), 2);
        assert!(EdgeKind::Value.admits(Skipped));
        assert!(!EdgeKind::Value.admits(Cancelled));
        assert_eq!(admit(EdgeKind::TerminalObservation), 4);
        // Spec 13 · `.cause` rides the SAME all-four pass-set as `.status`.
        assert_eq!(role_of_field(Some("cause")), EdgeKind::TerminalObservation);
        assert_eq!(admit(EdgeKind::FailureObservation), 2);
        assert!(EdgeKind::FailureObservation.admits(Skipped));
        assert!(EdgeKind::Control(AfterPredicate::Terminal).admits(Cancelled));
        assert!(!EdgeKind::Control(AfterPredicate::Success).admits(Skipped));
        assert!(EdgeKind::Control(AfterPredicate::Skipped).admits(Skipped));
        assert!(EdgeKind::Control(AfterPredicate::Failure).admits(Failure));
    }

    #[test]
    fn one_expression_two_refs_two_edges() {
        let yaml = format!(
            "{HEADER}tasks:
  a:
    exec: {{ command: [\"true\"] }}
  b:
    exec: {{ command: [\"true\"] }}
  c:
    with:
      both: \"${{{{ tasks.a.output }}}} and ${{{{ tasks.b.output }}}}\"
    exec: {{ command: [\"true\"] }}
"
        );
        let (edges, _) = edges_of(&yaml);
        assert_eq!(edges.len(), 2);
        assert_eq!((edges[0].from, edges[0].to), (0, 2));
        assert_eq!((edges[1].from, edges[1].to), (1, 2));
        assert!(edges.iter().all(|e| e.binding.as_deref() == Some("both")));
    }
}
