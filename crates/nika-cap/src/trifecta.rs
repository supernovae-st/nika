// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The lethal-trifecta check (NEP-0002 · `NIKA-SEC-009`) — the pure judge.
//!
//! The lethal trifecta (Willison) + the Agents Rule of Two (Meta) are the
//! two dominant agent-safety heuristics; NEP-0002 makes them DECIDABLE from
//! the declared capability boundary. Three legs, read off `permits:` and the
//! task graph (NEP-0002 §Specification · v2.0):
//!
//! - **① private-data** — `permits.fs.read` is non-empty.
//! - **② untrusted ingress** — the boundary grants an ingress-capable tool
//!   (`nika:fetch` covered by a glob · any `mcp:*` server) AND the task
//!   graph REALIZES one: a fetch invoked · an `mcp:*` invoked · an
//!   `agent:` whose whitelist admits ingress. Untrusted content propagates
//!   through `with:` bindings, `for_each` items, `tasks.*.output` reads and
//!   `on_error.recover` reads — and `infer:`/`agent:` outputs carry it when
//!   their prompt sees it (the integrity direction; the ADR-092 verbatim-
//!   echo carve-out is confidentiality-only and does not apply). First-party
//!   LOCAL builtins are NOT ingress (①/③'s domains).
//! - **③ external egress** — `permits.net.http` is non-empty, OR a declared
//!   `fs.write` glob escapes the workspace (absolute · `~` · a preserved
//!   leading `..` after lexical normalization — the [`crate::fit`] semantics),
//!   OR `permits.exec` is enabled.
//!
//! When ①∧②∧③ hold, every egress-capable task **the untrusted content
//! actually reaches** (a realized flow — v2.0, witness-named `source →
//! sink`) MUST be dominated by a blocking human gate — an `invoke:` of
//! [`crate::HUMAN_GATE_TOOL`] carrying NO `default:` arg (a default lets
//! the run proceed unattended; the Rule of Two escape is the HUMAN
//! decision, not a scripted fallback). **Dominance**, not mere ancestry: a
//! gate a sibling branch bypasses mitigates nothing. One violation per
//! ungated tainted egress task, message opening with the NEP's verbatim
//! « lethal trifecta complete · human gate required ».
//!
//! The judge is pure L0 (the same shape the dead `policy_violations`
//! set): it
//! reads projected [`TrifectaSubject`] rows, [`TaintWitness`] flow facts
//! and the declared boundary, never an AST. `nika_check` owns the
//! projection, the content-taint pass, and the DAG-validity gating (an
//! unanalyzable graph yields NO claim — skipped, never wrong).

use std::collections::BTreeSet;

use crate::Permits;
use crate::fit::lexically_normalize;

/// One task, projected for the judge (id · egress capability · gate-ness ·
/// direct predecessors in the derived graph — control `after:` and data
/// `with:` edges alike, the policy projection's ONE-edge-set doctrine: an
/// egress fed untrusted data through `with:` is exactly the threat).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TrifectaSubject {
    /// The task id (the witness a violation names).
    pub id: String,
    /// Whether the task can realize a boundary-permitted external effect:
    /// an `exec:` task · an `invoke:` with a net or fs-write builtin effect
    /// · an `mcp:` tool call (effects are server-side — the `tools:` grant
    /// is the boundary, fail-closed) · an `agent:` loop whose whitelist
    /// admits an egress-effecting tool. `infer:` is NOT egress — provider
    /// egress rides the engine's media plane, not `permits.net.http` (the
    /// effect.rs table's own doctrine).
    pub egress_capable: bool,
    /// Whether the task is a BLOCKING human gate (an `invoke:` of
    /// [`crate::HUMAN_GATE_TOOL`] with no `default:` arg).
    pub human_gate: bool,
    /// Whether the task is a REALIZED untrusted-content source (v2.0): an
    /// invoked `nika:fetch` · an invoked `mcp:*` whose catalog mark is not
    /// `trusted` (absent/unknown = untrusted, fail-closed) · an `agent:`
    /// whose whitelist admits an ingress-capable tool (a browsing agent's
    /// final message is attacker-influenced even with a clean prompt).
    /// v1's granted-but-never-invoked arming is gone: ② needs one of these.
    pub ingress_source: bool,
    /// Direct predecessors (indices into the same subject slice).
    pub parents: Vec<usize>,
}

impl TrifectaSubject {
    /// Construct a subject (the non-exhaustive-struct constructor ·
    /// forward-compat invariant #19). Parents are pushed by the caller's
    /// edge pass afterwards.
    #[must_use]
    pub fn new(id: String, egress_capable: bool, human_gate: bool) -> Self {
        Self {
            id,
            egress_capable,
            human_gate,
            ingress_source: false,
            parents: Vec::new(),
        }
    }

    /// Mark the subject an untrusted-content source (v2.0 builder — the
    /// projection sets it, the judge's ② reads it).
    #[must_use]
    pub fn with_ingress_source(mut self, ingress_source: bool) -> Self {
        self.ingress_source = ingress_source;
        self
    }
}

/// The realized-flow fact for one subject (v2.0): whether untrusted
/// content from an ingress source REACHES it (the projection computes it
/// over the data graph; the judge stays pure L0).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TaintWitness {
    /// Untrusted content reaches this subject's effect surface.
    pub tainted: bool,
    /// The ingress task the content originates from (the flow witness a
    /// violation names), when tainted.
    pub source: Option<String>,
}

impl TaintWitness {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(tainted: bool, source: Option<String>) -> Self {
        Self { tainted, source }
    }

    /// A clean subject (no realized flow).
    #[must_use]
    pub fn clean() -> Self {
        Self::new(false, None)
    }
}

/// One trifecta finding — the ungated egress task (the witness) + the
/// teaching detail (NEP-0002 message first, the fix second).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TrifectaViolation {
    /// The egress-capable task no blocking human gate dominates.
    pub task: String,
    /// The ingress task whose untrusted content reaches `task` (the
    /// realized-flow witness · `None` only for a v1-shaped caller).
    #[serde(default)]
    pub source: Option<String>,
    /// Human detail — opens with « lethal trifecta complete · human gate
    /// required » (NEP-0002 verbatim), then names the fix.
    pub detail: String,
}

/// An egress sink that WOULD have violated NEP-0002 and did not, because
/// a blocking gate dominates it.
///
/// The clearance below has always computed this and returned an empty
/// vec, so a trifecta cleared by a CREDITED GATE and one cleared by a
/// MISSING LEG were indistinguishable to every reader. They are not the
/// same fact: the second is safe by construction, the first is safe only
/// if the gate is real, and whether a gate is real is a question ANOTHER
/// lane answers (`nika-check::consent`, NEP-0020 · `NIKA-SEC-014`).
///
/// Measured 2026-08-20 on 0.111.0, one card, two lanes ·
///
/// ```text
/// ✔ TRIFECTA no lethal trifecta over the declared permits: without a human gate
/// ✖ CONSENT  [NIKA-SEC-014] task `leak` … the effect fires on 'no'
/// ```
///
/// Publishing the pair lets the card DERIVE its tick instead of asserting
/// it: a mitigation whose gate the consent lane refutes on the same sink
/// is not a mitigation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TrifectaMitigation {
    /// The egress-capable task the gate cleared.
    pub sink: String,
    /// The blocking gate that dominates it.
    pub gate: String,
}

/// Both halves of one clearance pass: what the trifecta REFUSES, and what
/// it CREDITED to a gate. A reader that sees only the first cannot tell a
/// safe file from a rubber-stamped one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TrifectaVerdict {
    /// The sinks no blocking gate dominates (`NIKA-SEC-009`).
    pub violations: Vec<TrifectaViolation>,
    /// The sinks a blocking gate cleared, each naming its gate.
    pub mitigations: Vec<TrifectaMitigation>,
}

/// A declared `fs.write` glob escapes the workspace (leg ③'s write variant):
/// absolute, home-anchored, or climbing out with a preserved leading `..`
/// after lexical normalization — a relative escape MUST stay visible (the
/// fit.rs doctrine: dropping it would collapse an out-of-boundary glob into
/// an in-boundary one).
fn write_glob_escapes(glob: &str) -> bool {
    if glob.starts_with('/')
        || glob.starts_with('~')
        || glob == "$HOME"
        || glob.starts_with("$HOME/")
        || glob == "${HOME}"
        || glob.starts_with("${HOME}/")
    {
        return true;
    }
    lexically_normalize(glob).starts_with("..")
}

/// A declared `tools:` grant admits untrusted-ingress content (leg ②'s
/// grant form) — the ONE predicate lives in [`crate::integrity`] (shared
/// check≡run): `nika:fetch` — a glob like `nika:*` covers it — or any
/// `mcp:*` server (server-provided content is untrusted by construction).
/// A negated entry (`!…`) ADMITS nothing and never counts; first-party
/// local builtins (`nika:read` · `nika:write` · …) are not ingress.
fn grants_untrusted_ingress(tools: &[String]) -> bool {
    tools
        .iter()
        .any(|g| crate::integrity::tool_grant_admits_ingress(g))
}

/// The three legs off the DECLARED boundary (NEP-0002 §Specification ·
/// v2.0: ② needs the grant AND one realized source — a granted-but-never
/// invoked ingress no longer arms the trifecta · F2 2026-07-30: the exec
/// permit is an ingress CHANNEL, the grant twin of `classify(exec) =
/// born_ingress` — a subprocess imports content the workflow did not
/// author, so `exec` allowed arms leg ② exactly like a fetch grant).
fn legs(permits: &Permits, subjects: &[TrifectaSubject]) -> (bool, bool, bool) {
    let private_read = permits.fs.as_ref().is_some_and(|fs| !fs.read.is_empty());
    let ingress_granted = permits.allows_exec()
        || permits
            .tools
            .as_ref()
            .is_some_and(|t| grants_untrusted_ingress(t));
    let untrusted_ingress = subjects.iter().any(|s| s.ingress_source) && ingress_granted;
    let external_egress = permits.net.as_ref().is_some_and(|n| !n.http.is_empty())
        || permits
            .fs
            .as_ref()
            .is_some_and(|fs| fs.write.iter().any(|g| write_glob_escapes(g)))
        || permits.allows_exec();
    (private_read, untrusted_ingress, external_egress)
}

/// WHY the three legs are legs HERE — one clause per leg, naming the
/// grant that arms it (#1399: a twenty-line local shell pipeline read as
/// an exfiltration scenario with nothing anywhere saying that an exec
/// output is untrusted ingress and an exec grant is external egress).
fn leg_reasons(permits: &Permits, subjects: &[TrifectaSubject]) -> String {
    let sources: Vec<String> = subjects
        .iter()
        .filter(|s| s.ingress_source)
        .map(|s| format!("`{}`", s.id))
        .collect();
    let mut ingress_by = Vec::new();
    if permits.allows_exec() {
        ingress_by.push("`permits.exec` (an exec output is untrusted content)");
    }
    if permits
        .tools
        .as_ref()
        .is_some_and(|t| grants_untrusted_ingress(t))
    {
        ingress_by.push("`permits.tools` admitting a fetch / MCP / browsing tool (its result is untrusted content)");
    }
    let mut egress_by = Vec::new();
    if permits.net.as_ref().is_some_and(|n| !n.http.is_empty()) {
        egress_by.push("`permits.net.http`");
    }
    if permits
        .fs
        .as_ref()
        .is_some_and(|fs| fs.write.iter().any(|g| write_glob_escapes(g)))
    {
        egress_by.push("an `fs.write` grant outside `./`");
    }
    if permits.allows_exec() {
        egress_by.push("`permits.exec` (a program reaches anywhere)");
    }
    format!(
        "legs here: private read = `permits.fs.read` · untrusted ingress = {} via {} · external egress via {}",
        if sources.is_empty() {
            "an ingress task".to_owned()
        } else {
            sources.join(", ")
        },
        ingress_by.join(" and "),
        egress_by.join(" and "),
    )
}

/// The ENTRY tasks feeding `sink` — its ancestors with no parents (or
/// `sink` itself when it has none). A blocking gate dominates every path
/// to the sink exactly when every one of these runs after it (#1394: the
/// first refusal said « upstream of the fetch task » while a `with:` data
/// edge from a private read reached the sink around that placement; the
/// judge had already computed the set, so it names it).
fn entry_ancestors(subjects: &[TrifectaSubject], sink: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![sink];
    let mut roots = BTreeSet::new();
    while let Some(i) = stack.pop() {
        if !seen.insert(i) {
            continue;
        }
        let Some(s) = subjects.get(i) else {
            continue;
        };
        if s.parents.is_empty() {
            roots.insert(i);
        }
        stack.extend(s.parents.iter().copied());
    }
    roots
        .into_iter()
        .filter_map(|i| subjects.get(i).map(|s| format!("`{}`", s.id)))
        .collect()
}

/// `a` · `a and b` · `a, b and c`.
fn join_names(names: &[String]) -> String {
    match names {
        [] => "the ingress".to_owned(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

/// The virtual-root marker in dominator sets (never a subject index).
const ROOT: usize = usize::MAX;

/// Judge the trifecta (v2.0). `witnesses[i]` is the realized-flow fact for
/// `subjects[i]` (the projection computes it — the judge stays pure L0);
/// `topo_order` is any topological order of the SAME derived graph the
/// subjects' `parents` index into, covering every subject exactly once (the
/// caller owns DAG validity — an unanalyzable graph yields no order and NO
/// claim, the IFC/policy gating precedent). Deterministic: violations
/// follow task declaration order.
#[must_use]
pub fn trifecta_verdict(
    permits: &Permits,
    subjects: &[TrifectaSubject],
    witnesses: &[TaintWitness],
    topo_order: &[usize],
) -> TrifectaVerdict {
    let (one, two, three) = legs(permits, subjects);
    if !(one && two && three) {
        // A leg is absent: nothing to clear, so nothing to credit. The
        // empty verdict here and the empty `violations` below are now
        // DIFFERENT values, which is the whole point.
        return TrifectaVerdict::default();
    }
    let dom = dominators(subjects, topo_order);
    let mut out = Vec::new();
    let mut mitigations = Vec::new();
    for (i, s) in subjects.iter().enumerate() {
        if !s.egress_capable {
            continue;
        }
        // v2.0 — the REALIZED trifecta: untrusted content must actually
        // reach this egress task (a flow witness), not merely be granted
        // somewhere in the workflow.
        let witness = witnesses.get(i);
        if !witness.is_some_and(|w| w.tainted) {
            continue;
        }
        // The dominating gate, NAMED. `any()` here answered "is there
        // one" and dropped WHICH — the card then had no way to ask
        // whether that gate is real.
        let gate = dom[i]
            .iter()
            .find_map(|&g| subjects.get(g).filter(|t| t.human_gate).map(|t| &t.id));
        if let Some(gate) = gate {
            mitigations.push(TrifectaMitigation {
                sink: s.id.clone(),
                gate: gate.clone(),
            });
        }
        if gate.is_none() {
            let source = witness.and_then(|w| w.source.clone());
            // The dominance rule, EXPLAINED (user gauntlet 2026-07-31 ·
            // G-07: the printed fix applied verbatim returned the finding
            // byte-identical, zero delta, no rule named). Two teaching
            // arms, one placement law: a gate parked next to the sink
            // cannot dominate it — the data edges feeding the sink from
            // pre-gate tasks are paths too. Upstream of the ingress,
            // EVERY path (order and data alike) crosses the gate.
            let why = bypass_note(&dom, subjects, i).unwrap_or_else(|| {
                format!(
                    "no blocking `invoke: {}` dominates every path to it",
                    crate::HUMAN_GATE_TOOL
                )
            });
            let legs = leg_reasons(permits, subjects);
            let entries = join_names(&entry_ancestors(subjects, i));
            out.push(TrifectaViolation {
                task: s.id.clone(),
                detail: format!(
                    "lethal trifecta complete · human gate required — untrusted content from \
                     `{}` reaches egress task `{}` while private read + untrusted ingress + \
                     external egress are all permitted ({legs}), and {why} · fix: a blocking \
                     `invoke: {}` must dominate EVERY path to `{}`, data edges (`with:`) \
                     included — place it upstream of {entries} (grant `{}` in `permits.tools` · \
                     bind its answer into the effect with `with:` · gate the effect with \
                     `when:`), then every path to `{}`, order and data edges alike, crosses \
                     it (NEP-0002 · the Rule of Two as a check)",
                    source.as_deref().unwrap_or("<ingress>"),
                    s.id,
                    crate::HUMAN_GATE_TOOL,
                    s.id,
                    crate::HUMAN_GATE_TOOL,
                    s.id,
                ),
                source,
            });
        }
    }
    TrifectaVerdict {
        violations: out,
        mitigations,
    }
}

/// Dominators over the derived DAG with a virtual root: dom(root)={root},
/// dom(entry)={root, entry}, dom(n)={n} ∪ ⋂ dom(parents). One pass in
/// topological order suffices — the DAG is acyclic BY CONSTRUCTION, so
/// every parent is settled before its child.
fn dominators(subjects: &[TrifectaSubject], topo_order: &[usize]) -> Vec<BTreeSet<usize>> {
    let mut dom: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); subjects.len()];
    for &i in topo_order {
        let Some(subject) = subjects.get(i) else {
            continue;
        };
        let mut d = if subject.parents.is_empty() {
            BTreeSet::from([ROOT])
        } else {
            let mut acc: Option<BTreeSet<usize>> = None;
            for &p in &subject.parents {
                let pd = dom[p].clone();
                acc = Some(match acc {
                    None => pd,
                    Some(a) => a.intersection(&pd).copied().collect(),
                });
            }
            acc.unwrap_or_else(|| BTreeSet::from([ROOT]))
        };
        d.insert(i);
        dom[i] = d;
    }
    dom
}

/// Name WHY an existing gate fails to dominate the sink (the dominance
/// rule spoken, not implied): the first declared gate + the first
/// parent of the sink whose paths never cross it — usually the data
/// edge (`with: ${{ tasks.X.output }}`) feeding the sink from a
/// pre-gate task. `None` when the workflow has no gate at all (the
/// generic clause reads better than a vacuous explanation).
fn bypass_note(
    dom: &[BTreeSet<usize>],
    subjects: &[TrifectaSubject],
    sink: usize,
) -> Option<String> {
    let gate = subjects.iter().position(|t| t.human_gate)?;
    let bypass = subjects[sink]
        .parents
        .iter()
        .find(|&&p| !dom.get(p).is_some_and(|d| d.contains(&gate)))?;
    // « its parent », not « the edge from »: the source of the untrusted
    // content and the parent that bypasses the gate are two different
    // roles, and a sentence that named both as bare task ids read as a
    // finding contradicting itself (#1399).
    Some(format!(
        "the blocking gate `{}` does not dominate `{}` — its parent `{}` reaches it \
         without crossing the gate (a `with:` data edge is a path too)",
        subjects[gate].id, subjects[sink].id, subjects[*bypass].id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The credit half, in isolation: a cleared trifecta must NAME the
    /// gate that cleared it, and a trifecta with a missing leg must
    /// credit nothing.
    ///
    /// Both used to return the same empty vec, which is how a card could
    /// print the same tick for "safe by construction" and "safe if this
    /// prompt is real" without being able to tell them apart.
    #[test]
    fn a_cleared_trifecta_names_the_gate_that_cleared_it() {
        // gate → ingress → egress · every path crosses the gate
        let mut subjects = vec![
            TrifectaSubject::new("ask".to_owned(), false, true),
            TrifectaSubject::new("fetch".to_owned(), false, false).with_ingress_source(true),
            TrifectaSubject::new("leak".to_owned(), true, false),
        ];
        subjects[1].parents = vec![0];
        subjects[2].parents = vec![1];
        let witnesses = vec![
            TaintWitness::new(false, None),
            TaintWitness::new(false, None),
            TaintWitness::new(true, Some("fetch".to_owned())),
        ];
        let v = trifecta_verdict(&full_boundary(), &subjects, &witnesses, &topo(&subjects));
        assert!(
            v.violations.is_empty(),
            "the gate dominates the sink: {:?}",
            v.violations
        );
        assert_eq!(
            v.mitigations,
            vec![TrifectaMitigation {
                sink: "leak".to_owned(),
                gate: "ask".to_owned(),
            }],
            "the clearance must name its gate"
        );

        // the SAME shape with a leg removed: nothing to clear, so nothing
        // to credit — a different fact, and now a different value.
        let mut p = full_boundary();
        p.net = None;
        let v = trifecta_verdict(&p, &subjects, &witnesses, &topo(&subjects));
        assert!(v.violations.is_empty() && v.mitigations.is_empty());
    }

    /// The violations half, for batteries whose subject is the refusal.
    /// The credit half is exercised by its own test below, so neither
    /// reads through the other's noise.
    fn trifecta_violations(
        permits: &Permits,
        subjects: &[TrifectaSubject],
        witnesses: &[TaintWitness],
        topo_order: &[usize],
    ) -> Vec<TrifectaViolation> {
        trifecta_verdict(permits, subjects, witnesses, topo_order).violations
    }

    use crate::{ExecPermit, FsPermits, NetPermits};

    fn boundary(read: &[&str], write: &[&str], http: &[&str], tools: &[&str]) -> Permits {
        Permits {
            fs: Some(FsPermits::new(
                read.iter().map(ToString::to_string).collect(),
                write.iter().map(ToString::to_string).collect(),
            )),
            net: Some(NetPermits::new(
                http.iter().map(ToString::to_string).collect(),
            )),
            exec: None,
            tools: Some(tools.iter().map(ToString::to_string).collect()),
            env: None,
        }
    }

    fn full_boundary() -> Permits {
        boundary(
            &["./private/**"],
            &["./out/**"],
            &["api.example.com"],
            &["nika:fetch"],
        )
    }

    fn topo(subjects: &[TrifectaSubject]) -> Vec<usize> {
        // Kahn on the projected parents — the tests build tiny DAGs.
        let mut done = vec![false; subjects.len()];
        let mut order = Vec::new();
        while order.len() < subjects.len() {
            for (i, s) in subjects.iter().enumerate() {
                if !done[i] && s.parents.iter().all(|&p| done[p]) {
                    done[i] = true;
                    order.push(i);
                }
            }
        }
        order
    }

    fn ingress_subjects() -> Vec<TrifectaSubject> {
        vec![TrifectaSubject::new("fetch".to_owned(), true, false).with_ingress_source(true)]
    }

    /// A fetch → leak pair: both egress-capable, fetch the realized
    /// ingress source, leak tainted by it — the canonical v2.0 trifecta.
    fn realized_pair() -> (Vec<TrifectaSubject>, Vec<TaintWitness>) {
        let fetch =
            TrifectaSubject::new("fetch_page".to_owned(), true, false).with_ingress_source(true);
        let mut leak = TrifectaSubject::new("leak".to_owned(), true, false);
        leak.parents.push(0);
        let subjects = vec![fetch, leak];
        let witnesses = vec![
            TaintWitness::clean(),
            TaintWitness::new(true, Some("fetch_page".to_owned())),
        ];
        (subjects, witnesses)
    }

    #[test]
    fn legs_read_off_the_declared_boundary() {
        let (a, b, c) = legs(&full_boundary(), &ingress_subjects());
        assert!((a, b, c) == (true, true, true), "all three legs declared");
        // ① falls with an empty read set.
        let mut p = full_boundary();
        p.fs = Some(FsPermits::new(Vec::new(), vec!["./out/**".to_owned()]));
        assert!(
            !legs(&p, &ingress_subjects()).0,
            "① falls with an empty read set"
        );
        // ② falls with no ingress-capable grant…
        p = full_boundary();
        p.tools = Some(Vec::new());
        assert!(
            !legs(&p, &ingress_subjects()).1,
            "② falls with no tools grant"
        );
        // …and with no REALIZED source either (v2.0's conjunction).
        assert!(
            !legs(
                &full_boundary(),
                &[TrifectaSubject::new("x".to_owned(), true, false)]
            )
            .1,
            "② falls with no realized source"
        );
        // ③ falls with no net, no escaping write, no exec.
        let mut p = boundary(&["./private/**"], &["./out/**"], &[], &["nika:fetch"]);
        assert!(
            !legs(&p, &ingress_subjects()).2,
            "③ falls with no egress channel"
        );
        // …and ANY of the three variants re-arms it.
        p.exec = Some(ExecPermit::Any);
        assert!(legs(&p, &ingress_subjects()).2, "exec enabled is egress");
    }

    #[test]
    fn first_party_local_tools_are_not_ingress() {
        // The spec's own permits-fit fixture (conformance deep/014) —
        // private read + workspace write + an exec allowlist + LOCAL
        // builtins only: two legs, never three (the v1.1 grant refinement,
        // kept verbatim in v2.0).
        let mut p = Permits::new();
        p.fs = Some(FsPermits::new(
            vec!["./data/**".to_owned()],
            vec!["./out/**".to_owned()],
        ));
        p.exec = Some(ExecPermit::Programs(vec!["git".to_owned()]));
        p.tools = Some(vec!["nika:read".to_owned(), "nika:write".to_owned()]);
        let subjects = vec![TrifectaSubject::new("log_head".to_owned(), true, false)];
        let (one, two, three) = legs(&p, &subjects);
        assert!(
            one && !two && three,
            "local builtins drop ② — the fixture stays VALID: {one} {two} {three}"
        );
        let witnesses = vec![TaintWitness::new(true, Some("x".to_owned()))];
        assert!(
            trifecta_violations(&p, &subjects, &witnesses, &[0]).is_empty(),
            "② dropped → no finding even with a tainted ungated egress task"
        );
    }

    #[test]
    fn ingress_grants_are_fetch_globs_and_mcp() {
        // `nika:*` covers fetch → ② holds.
        let p = boundary(&["./private/**"], &["./out/**"], &[], &["nika:*"]);
        assert!(
            legs(&p, &ingress_subjects()).1,
            "a nika:* grant covers nika:fetch"
        );
        // Any mcp:* server is untrusted content by construction.
        let p = boundary(&["./private/**"], &["./out/**"], &[], &["mcp:browser/*"]);
        assert!(legs(&p, &ingress_subjects()).1, "an mcp grant is ingress");
        // A negation-only list ADMITS nothing → ② falls.
        let p = boundary(
            &["./private/**"],
            &["./out/**"],
            &[],
            &["!mcp:x", "!nika:fetch"],
        );
        assert!(!legs(&p, &ingress_subjects()).1, "negations admit nothing");
        // A first-party grant outside fetch/mcp is not ingress.
        let p = boundary(
            &["./private/**"],
            &["./out/**"],
            &[],
            &["nika:connectome/*"],
        );
        assert!(
            !legs(&p, &ingress_subjects()).1,
            "connectome is first-party memory, not ingress"
        );
    }

    #[test]
    fn write_glob_escape_variants() {
        assert!(write_glob_escapes("/etc/passwd"), "absolute escapes");
        assert!(write_glob_escapes("~/loot.txt"), "home escapes");
        assert!(write_glob_escapes("$HOME/loot.txt"), "$HOME escapes");
        assert!(
            write_glob_escapes("${HOME}/loot.txt"),
            "dollar-brace HOME escapes"
        );
        assert!(write_glob_escapes("../outside.txt"), "climb escapes");
        assert!(write_glob_escapes("./out/../../etc/x"), "normalized climb");
        assert!(!write_glob_escapes("./out/**"), "workspace write stays");
        assert!(!write_glob_escapes("out/report.txt"), "bare relative stays");
    }

    #[test]
    fn trifecta_complete_refuses_every_tainted_ungated_egress() {
        let (subjects, witnesses) = realized_pair();
        let v = trifecta_violations(&full_boundary(), &subjects, &witnesses, &topo(&subjects));
        assert_eq!(v.len(), 1, "one finding per tainted ungated egress: {v:?}");
        assert_eq!(v[0].task, "leak");
        assert_eq!(v[0].source.as_deref(), Some("fetch_page"));
        assert!(
            v[0].detail
                .starts_with("lethal trifecta complete · human gate required"),
            "NEP-0002 message verbatim first: {}",
            v[0].detail
        );
        assert!(
            v[0].detail
                .contains("`fetch_page` reaches egress task `leak`"),
            "the flow witness is named: {}",
            v[0].detail
        );
    }

    #[test]
    fn an_untainted_egress_is_clean_even_ungated() {
        // v2.0 precision: egress capability WITHOUT a realized flow is not
        // a trifecta (the deploy-exec on a parallel branch, far from the
        // fetch — clean; v1.1 fired here).
        let (subjects, _) = realized_pair();
        let witnesses = vec![TaintWitness::clean(), TaintWitness::clean()];
        assert!(
            trifecta_violations(&full_boundary(), &subjects, &witnesses, &topo(&subjects))
                .is_empty(),
            "no realized flow → no finding"
        );
    }

    #[test]
    fn two_of_three_legs_is_clean() {
        let (subjects, witnesses) = realized_pair();
        // Drop ①.
        let p = boundary(&[], &["./out/**"], &["api.example.com"], &["nika:fetch"]);
        assert!(trifecta_violations(&p, &subjects, &witnesses, &topo(&subjects)).is_empty());
        // Drop ② (no fetch grant).
        let p = boundary(&["./private/**"], &["./out/**"], &["api.example.com"], &[]);
        assert!(trifecta_violations(&p, &subjects, &witnesses, &topo(&subjects)).is_empty());
        // Drop ③ (no net · workspace-confined writes · no exec).
        let p = boundary(&["./private/**"], &["./out/**"], &[], &["nika:fetch"]);
        assert!(trifecta_violations(&p, &subjects, &witnesses, &topo(&subjects)).is_empty());
    }

    #[test]
    fn a_dominating_gate_disarms_the_trifecta() {
        // ask (gate) → fetch_page → leak : every path to egress crosses ask.
        let mut fetch =
            TrifectaSubject::new("fetch_page".to_owned(), true, false).with_ingress_source(true);
        fetch.parents.push(0);
        let mut leak = TrifectaSubject::new("leak".to_owned(), true, false);
        leak.parents.push(1);
        let subjects = vec![
            TrifectaSubject::new("ask".to_owned(), false, true),
            fetch,
            leak,
        ];
        let witnesses = vec![
            TaintWitness::clean(),
            TaintWitness::clean(),
            TaintWitness::new(true, Some("fetch_page".to_owned())),
        ];
        let v = trifecta_violations(&full_boundary(), &subjects, &witnesses, &topo(&subjects));
        assert!(v.is_empty(), "the gate dominates every egress path: {v:?}");
    }

    #[test]
    fn a_bypassable_gate_is_no_gate() {
        // Diamond: ask gates the LEFT branch; leak runs on the RIGHT branch
        // with no gate ancestor — ancestry without dominance mitigates nothing.
        let mut left = TrifectaSubject::new("left_egress".to_owned(), true, false);
        left.parents.push(0);
        let subjects = vec![
            TrifectaSubject::new("ask".to_owned(), false, true),
            left,
            TrifectaSubject::new("leak".to_owned(), true, false).with_ingress_source(true),
        ];
        let witnesses = vec![
            TaintWitness::clean(),
            TaintWitness::clean(),
            TaintWitness::new(true, Some("fetch_page".to_owned())),
        ];
        let v = trifecta_violations(&full_boundary(), &subjects, &witnesses, &topo(&subjects));
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].task, "leak");
        assert!(
            v[0].detail
                .contains("no blocking `invoke: nika:prompt` dominates")
        );
    }

    #[test]
    fn no_egress_task_means_no_surface() {
        // Wide-open boundary, but every task is pure compute: the trifecta
        // has no egress task to fire on (vacuously gated — even tainted).
        let subjects = vec![TrifectaSubject::new("think".to_owned(), false, false)];
        let witnesses = vec![TaintWitness::new(true, Some("x".to_owned()))];
        assert!(
            trifecta_violations(&full_boundary(), &subjects, &witnesses, &[0]).is_empty(),
            "no egress-capable task · no finding"
        );
    }
}
