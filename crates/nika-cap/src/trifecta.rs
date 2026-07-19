// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The lethal-trifecta check (NEP-0002 · `NIKA-SEC-009`) — the pure judge.
//!
//! The lethal trifecta (Willison) + the Agents Rule of Two (Meta) are the
//! two dominant agent-safety heuristics; NEP-0002 makes them DECIDABLE from
//! the declared capability boundary. Three legs, read off `permits:` and the
//! task graph (NEP-0002 §Specification — v1 deliberately coarse):
//!
//! - **① private-data** — `permits.fs.read` is non-empty.
//! - **② untrusted ingress** — a `nika:fetch` builtin is invoked, OR
//!   `permits.tools` grants an ingress-capable tool: `nika:fetch` (a glob
//!   like `nika:*` covers it) or any `mcp:*` server (server-provided
//!   content is untrusted by construction). First-party LOCAL builtins
//!   (`nika:read` · `nika:write` · …) are NOT ingress — a private read is
//!   ①'s domain, a write ③'s (v1.1: the coarse any-grant reading flagged
//!   the spec's own permits-fit fixture deep/014; per-tool trust marks
//!   remain the documented v2 refinement).
//! - **③ external egress** — `permits.net.http` is non-empty, OR a declared
//!   `fs.write` glob escapes the workspace (absolute · `~` · a preserved
//!   leading `..` after lexical normalization — the [`crate::fit`] semantics),
//!   OR `permits.exec` is enabled.
//!
//! When ①∧②∧③ hold, every egress-capable task MUST be dominated by a
//! blocking human gate — an `invoke:` of [`crate::HUMAN_GATE_TOOL`] carrying
//! NO `default:` arg (a default lets the run proceed unattended; the Rule of
//! Two escape is the HUMAN decision, not a scripted fallback). **Dominance**,
//! not mere ancestry: a gate a sibling branch bypasses mitigates nothing
//! (« dominates every egress-capable task on that path »). One violation per
//! ungated egress task, message opening with the NEP's verbatim
//! « lethal trifecta complete · human gate required ».
//!
//! The judge is pure L0 (the [`crate::policy_violations`] precedent): it
//! reads projected [`TrifectaSubject`] rows + the declared boundary, never
//! an AST. `nika_schema::check` owns the projection and the DAG-validity
//! gating (an unanalyzable graph yields NO claim — skipped, never wrong).

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
    /// is the boundary, fail-closed) · an `agent:` loop with a non-empty
    /// tools whitelist. `infer:` is NOT egress — provider egress rides the
    /// engine's media plane, not `permits.net.http` (the effect.rs table's
    /// own doctrine).
    pub egress_capable: bool,
    /// Whether the task is a BLOCKING human gate (an `invoke:` of
    /// [`crate::HUMAN_GATE_TOOL`] with no `default:` arg).
    pub human_gate: bool,
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
            parents: Vec::new(),
        }
    }
}

/// One trifecta finding — the ungated egress task (the witness) + the
/// teaching detail (NEP-0002 message first, the fix second).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TrifectaViolation {
    /// The egress-capable task no blocking human gate dominates.
    pub task: String,
    /// Human detail — opens with « lethal trifecta complete · human gate
    /// required » (NEP-0002 verbatim), then names the fix.
    pub detail: String,
}

/// A declared `fs.write` glob escapes the workspace (leg ③'s write variant):
/// absolute, home-anchored, or climbing out with a preserved leading `..`
/// after lexical normalization — a relative escape MUST stay visible (the
/// fit.rs doctrine: dropping it would collapse an out-of-boundary glob into
/// an in-boundary one).
fn write_glob_escapes(glob: &str) -> bool {
    if glob.starts_with('/') || glob.starts_with('~') {
        return true;
    }
    lexically_normalize(glob).starts_with("..")
}

/// A declared `tools:` grant admits untrusted-ingress content (leg ②'s
/// grant form): `nika:fetch` — a glob like `nika:*` covers it — or any
/// `mcp:*` server (server-provided content is untrusted by construction).
/// A negated entry (`!…`) ADMITS nothing and never counts; first-party
/// local builtins (`nika:read` · `nika:write` · …) are not ingress.
fn grants_untrusted_ingress(tools: &[String]) -> bool {
    tools.iter().any(|g| {
        if g.starts_with('!') {
            return false;
        }
        g.starts_with("mcp:") || crate::permits::glob_matches(g, "nika:fetch")
    })
}

/// The three legs off the DECLARED boundary (NEP-0002 §Specification).
fn legs(permits: &Permits, uses_fetch: bool) -> (bool, bool, bool) {
    let private_read = permits.fs.as_ref().is_some_and(|fs| !fs.read.is_empty());
    let untrusted_ingress = uses_fetch
        || permits
            .tools
            .as_ref()
            .is_some_and(|t| grants_untrusted_ingress(t));
    let external_egress = permits.net.as_ref().is_some_and(|n| !n.http.is_empty())
        || permits
            .fs
            .as_ref()
            .is_some_and(|fs| fs.write.iter().any(|g| write_glob_escapes(g)))
        || permits.allows_exec();
    (private_read, untrusted_ingress, external_egress)
}

/// The virtual-root marker in dominator sets (never a subject index).
const ROOT: usize = usize::MAX;

/// Judge the trifecta. `topo_order` is any topological order of the SAME
/// derived graph the subjects' `parents` index into, covering every subject
/// exactly once (the caller owns DAG validity — an unanalyzable graph yields
/// no order and NO claim, the IFC/policy gating precedent). Deterministic:
/// violations follow task declaration order.
#[must_use]
pub fn trifecta_violations(
    permits: &Permits,
    uses_fetch: bool,
    subjects: &[TrifectaSubject],
    topo_order: &[usize],
) -> Vec<TrifectaViolation> {
    let (one, two, three) = legs(permits, uses_fetch);
    if !(one && two && three) {
        return Vec::new();
    }
    // Dominators over the derived DAG with a virtual root: dom(root)={root},
    // dom(entry)={root, entry}, dom(n)={n} ∪ ⋂ dom(parents). One pass in
    // topological order suffices — the DAG is acyclic BY CONSTRUCTION, so
    // every parent is settled before its child.
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
    let mut out = Vec::new();
    for (i, s) in subjects.iter().enumerate() {
        if !s.egress_capable {
            continue;
        }
        let gated = dom[i]
            .iter()
            .any(|&g| subjects.get(g).is_some_and(|t| t.human_gate));
        if !gated {
            out.push(TrifectaViolation {
                task: s.id.clone(),
                detail: format!(
                    "lethal trifecta complete · human gate required — task `{}` reaches \
                     egress while private read + untrusted ingress + external egress are all \
                     permitted, and no blocking `invoke: {}` dominates every path to it · fix: \
                     gate the egress path behind a human prompt task (NEP-0002 · the Rule of \
                     Two as a check)",
                    s.id,
                    crate::HUMAN_GATE_TOOL
                ),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn legs_read_off_the_declared_boundary() {
        let (a, b, c) = legs(&full_boundary(), false);
        assert!((a, b, c) == (true, true, true), "all three legs declared");
        // ① falls with an empty read set.
        let mut p = full_boundary();
        p.fs = Some(FsPermits::new(Vec::new(), vec!["./out/**".to_owned()]));
        assert!(!legs(&p, false).0, "① falls with an empty read set");
        // ② falls with no fetch and no tools.
        p = full_boundary();
        p.tools = Some(Vec::new());
        assert!(!legs(&p, false).1, "② falls with no fetch and no tools");
        // …but a fetch task alone re-arms it (the NEP's first ② form).
        assert!(legs(&p, true).1, "a fetch task re-arms ②");
        // ③ falls with no net, no escaping write, no exec.
        p = boundary(&["./private/**"], &["./out/**"], &[], &["nika:fetch"]);
        assert!(!legs(&p, false).2, "③ falls with no egress channel");
        // …and ANY of the three variants re-arms it.
        p.exec = Some(ExecPermit::Any);
        assert!(legs(&p, false).2, "exec enabled is egress");
    }

    #[test]
    fn first_party_local_tools_are_not_ingress() {
        // The spec's own permits-fit fixture (conformance deep/014) —
        // private read + workspace write + an exec allowlist + LOCAL
        // builtins only: two legs, never three (the v1.1 refinement).
        let p = Permits {
            fs: Some(FsPermits::new(
                vec!["./data/**".to_owned()],
                vec!["./out/**".to_owned()],
            )),
            net: None,
            exec: Some(ExecPermit::Programs(vec!["git".to_owned()])),
            tools: Some(vec!["nika:read".to_owned(), "nika:write".to_owned()]),
        };
        let (one, two, three) = legs(&p, false);
        assert!(
            one && !two && three,
            "local builtins drop ② — the fixture stays VALID: {one} {two} {three}"
        );
        assert!(
            trifecta_violations(
                &p,
                false,
                &[TrifectaSubject::new("log_head".to_owned(), true, false)],
                &[0]
            )
            .is_empty(),
            "② dropped → no finding even with an ungated egress task"
        );
    }

    #[test]
    fn ingress_grants_are_fetch_globs_and_mcp() {
        // `nika:*` covers fetch → ② holds.
        let p = boundary(&["./private/**"], &["./out/**"], &[], &["nika:*"]);
        assert!(legs(&p, false).1, "a nika:* grant covers nika:fetch");
        // Any mcp:* server is untrusted content by construction.
        let p = boundary(&["./private/**"], &["./out/**"], &[], &["mcp:browser/*"]);
        assert!(legs(&p, false).1, "an mcp grant is ingress");
        // A negation-only list ADMITS nothing → ② falls.
        let p = boundary(
            &["./private/**"],
            &["./out/**"],
            &[],
            &["!mcp:x", "!nika:fetch"],
        );
        assert!(!legs(&p, false).1, "negations admit nothing");
        // A first-party grant outside fetch/mcp is not ingress.
        let p = boundary(
            &["./private/**"],
            &["./out/**"],
            &[],
            &["nika:connectome/*"],
        );
        assert!(
            !legs(&p, false).1,
            "connectome is first-party memory, not ingress"
        );
    }

    #[test]
    fn write_glob_escape_variants() {
        assert!(write_glob_escapes("/etc/passwd"), "absolute escapes");
        assert!(write_glob_escapes("~/loot.txt"), "home escapes");
        assert!(write_glob_escapes("../outside.txt"), "climb escapes");
        assert!(write_glob_escapes("./out/../../etc/x"), "normalized climb");
        assert!(!write_glob_escapes("./out/**"), "workspace write stays");
        assert!(!write_glob_escapes("out/report.txt"), "bare relative stays");
    }

    #[test]
    fn trifecta_complete_refuses_every_ungated_egress() {
        let subjects = vec![
            TrifectaSubject::new("fetch_page".to_owned(), true, false),
            TrifectaSubject::new("leak".to_owned(), true, false),
        ];
        let v = trifecta_violations(&full_boundary(), true, &subjects, &topo(&subjects));
        assert_eq!(v.len(), 2, "one finding per ungated egress task: {v:?}");
        assert!(
            v[0].detail
                .starts_with("lethal trifecta complete · human gate required"),
            "NEP-0002 message verbatim first: {}",
            v[0].detail
        );
    }

    #[test]
    fn two_of_three_legs_is_clean() {
        let subjects = vec![TrifectaSubject::new("act".to_owned(), true, false)];
        // Drop ①.
        let p = boundary(&[], &["./out/**"], &["api.example.com"], &["nika:fetch"]);
        assert!(trifecta_violations(&p, true, &subjects, &[0]).is_empty());
        // Drop ② (no fetch, no tools).
        let p = boundary(&["./private/**"], &["./out/**"], &["api.example.com"], &[]);
        assert!(trifecta_violations(&p, false, &subjects, &[0]).is_empty());
        // Drop ③ (no net · workspace-confined writes · no exec).
        let p = boundary(&["./private/**"], &["./out/**"], &[], &["nika:fetch"]);
        assert!(trifecta_violations(&p, true, &subjects, &[0]).is_empty());
    }

    #[test]
    fn a_dominating_gate_disarms_the_trifecta() {
        // ask (gate) → fetch_page → leak : every path to egress crosses ask.
        let mut fetch = TrifectaSubject::new("fetch_page".to_owned(), true, false);
        fetch.parents.push(0);
        let mut leak = TrifectaSubject::new("leak".to_owned(), true, false);
        leak.parents.push(1);
        let subjects = vec![
            TrifectaSubject::new("ask".to_owned(), false, true),
            fetch,
            leak,
        ];
        let v = trifecta_violations(&full_boundary(), true, &subjects, &topo(&subjects));
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
            TrifectaSubject::new("leak".to_owned(), true, false),
        ];
        let v = trifecta_violations(&full_boundary(), true, &subjects, &topo(&subjects));
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
        // has no egress task to fire on (vacuously gated).
        let subjects = vec![TrifectaSubject::new("think".to_owned(), false, false)];
        assert!(
            trifecta_violations(&full_boundary(), true, &subjects, &[0]).is_empty(),
            "no egress-capable task · no finding"
        );
    }
}
