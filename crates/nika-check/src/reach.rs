// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `when:`-gate reachability — dead tasks proven before a token is spent
//! (ADR-092 ladder #6 · the no-SMT slice).
//!
//! Because the DAG is acyclic BY CONSTRUCTION, reachability is cheap:
//! per Prinz, Schwanen & van der Aalst 2026 *Deciding Reachability and
//! the Covering Problem with Diagnostics for Sound Acyclic Free-Choice
//! Workflow Nets* (arxiv.org/abs/2602.02447 · Feb 2026) the acyclic
//! free-choice class admits quadratic decision WITH diagnostics, while
//! the general problem is EXPSPACE-complete (Blondin, Mazowiecki &
//! Offtermatt 2022 *The complexity of soundness in workflow nets* ·
//! arxiv.org/abs/2201.05588). Nika's acyclicity is exactly what buys
//! tractability — no Z3, no state-space explosion.
//!
//! Method: ONE topological pass of abstract interpretation over the
//! terminal-status domain (spec `03-dag.md` §Task states · the four
//! `when:`-observable outcomes `success · failure · skipped ·
//! cancelled`), then per-gate EXACT enumeration over the referenced
//! upstream status sets in Kleene three-valued logic (non-status atoms
//! evaluate Unknown — sound: an Unknown gate is never declared dead).
//!
//! Soundness of the dead-claim: the per-task possible-status sets
//! over-approximate the reachable joint assignments (independent
//! product ⊇ true set, and `cancelled` is always possible — operator
//! stop · timeout · budget kill). A gate false under EVERY assignment
//! of the over-approximation is false under every real run.
//!
//! Two finding kinds ship in this slice ·
//! - **dead task** — the gate is unsatisfiable (contradiction ·
//!   impossible status · upstream-dead cascade). The task can never
//!   run; everything the author intended downstream is unreachable.
//! - **bad status literal** — a status atom compares against a string
//!   outside the spec vocabulary (`'failed'` for `'failure'` is the
//!   wild-caught class) — `==` never matches · `!=` always holds.

use std::collections::{BTreeMap, BTreeSet};

use nika_schema::expression::{Expr, Literal, RelOp, scan_templates};
use nika_schema::raw::RawWorkflow;
use nika_schema::types::{OnErrorAction, WhenGate};
use nika_types::suggest::damerau_levenshtein;

use super::ByteSpan;

/// The `when:`-observable terminal statuses (spec `03-dag.md` §Task
/// states — `pending`/`running` are non-terminal and never observable
/// by a gate, which evaluates once all deps are terminal).
pub const STATUS_VOCAB: [&str; 4] = ["success", "failure", "skipped", "cancelled"];

/// Bit per terminal status.
const S_SUCCESS: u8 = 1;
const S_FAILURE: u8 = 2;
const S_SKIPPED: u8 = 4;
const S_CANCELLED: u8 = 8;
const S_ALL: u8 = S_SUCCESS | S_FAILURE | S_SKIPPED | S_CANCELLED;

/// Gates referencing more distinct tasks than this are not enumerated
/// (4^6 = 4096 evaluations max per gate) — treated satisfiable, which
/// is the sound direction.
const MAX_GATE_REFS: usize = 6;

/// A gate whose `in [...]` list is longer than this is not enumerated. Each
/// of the up-to-`4^MAX_GATE_REFS` leaf evaluations re-scans the list, so a
/// huge list is O(4096 × len) per gate (a 3.6 MiB gate ≈ 0.9 s · Gate-11 F2).
/// A status list has ≤ 4 meaningful values, so a larger one is adversarial
/// padding — the gate widens to satisfiable∧falsifiable (the same sound
/// back-off as [`MAX_GATE_REFS`]).
const MAX_GATE_LIST_ITEMS: usize = 256;

/// What a gate-reachability finding is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GateFindingKind {
    /// The task's `when:` gate is unsatisfiable — the task NEVER runs.
    DeadTask,
    /// A status comparison uses a literal outside the spec vocabulary.
    BadStatusLiteral,
}

impl GateFindingKind {
    /// The spec error code this finding refuses under (05-errors ·
    /// one-voice: every check refusal names its code).
    #[must_use]
    pub fn wire_code(self) -> &'static str {
        match self {
            Self::DeadTask => "NIKA-DAG-006",
            Self::BadStatusLiteral => "NIKA-DAG-007",
        }
    }
}

/// One reachability finding (ADR-092 #6).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub struct GateFinding {
    /// The task whose gate is at fault.
    pub task: String,
    /// Dead gate or bad vocabulary.
    pub kind: GateFindingKind,
    /// Human diagnostic (the WHY — the paper's « with diagnostics »).
    pub detail: String,
    /// Machine-applicable fix, when one exists.
    pub fix: Option<String>,
    /// The gate's source span.
    pub span: Option<ByteSpan>,
}

/// Kleene three-valued logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum K3 {
    True,
    False,
    Unknown,
}

impl K3 {
    pub(crate) fn negate(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
    pub(crate) fn and(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }
    pub(crate) fn or(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }
}

/// The closest vocabulary word, with a CLOSED-VOCABULARY threshold:
/// the rustc-style `max(len/3, 1)` bound is tuned for open identifier
/// spaces and rejects `'failed'` → `'failure'` (distance 3); over a
/// fixed 4-word vocabulary a distance ≤ 3 suggestion is still
/// unambiguous and exactly the wild-caught class.
fn closest_status(lit: &str) -> Option<&'static str> {
    STATUS_VOCAB
        .iter()
        .map(|cand| (damerau_levenshtein(lit, cand), *cand))
        .filter(|(d, _)| *d <= 3)
        .min_by_key(|(d, _)| *d)
        .map(|(_, cand)| cand)
}

fn status_bit(lit: &str) -> Option<u8> {
    match lit {
        "success" => Some(S_SUCCESS),
        "failure" => Some(S_FAILURE),
        "skipped" => Some(S_SKIPPED),
        "cancelled" => Some(S_CANCELLED),
        _ => None,
    }
}

/// The with-binding names that resolve to a producer's STATUS — a
/// binding whose value is exactly one `${{ tasks.<id>.status }}` island
/// (W2 · the when: surface reads `with.<name>`, the binding carries the
/// observation · spec 03 §after pairing).
type StatusBindings = BTreeMap<String, String>;

/// A status observation → the observed task id · either directly
/// (`tasks.<id>.status` — legal on `on_finally` surfaces) or through a
/// status-carrying `with:` binding (`with.<name>` · the W2 idiom).
fn status_ref<'e>(e: &'e Expr, bindings: &'e StatusBindings) -> Option<&'e str> {
    match e {
        Expr::Member { base, field } if field == "status" => match base.as_ref() {
            Expr::Member { base, field } => match base.as_ref() {
                Expr::Ident(root) if root == "tasks" => Some(field),
                _ => None,
            },
            Expr::Index { base, index } => match (base.as_ref(), index.as_ref()) {
                (Expr::Ident(root), Expr::Lit(Literal::Str(id))) if root == "tasks" => Some(id),
                _ => None,
            },
            _ => None,
        },
        // `with.<name>` — resolves through the status-binding map.
        Expr::Member { base, field } => match base.as_ref() {
            Expr::Ident(root) if root == "with" => bindings.get(field).map(String::as_str),
            _ => None,
        },
        Expr::Index { base, index } => match (base.as_ref(), index.as_ref()) {
            (Expr::Ident(root), Expr::Lit(Literal::Str(name))) if root == "with" => {
                bindings.get(name).map(String::as_str)
            }
            _ => None,
        },
        _ => None,
    }
}

/// The either-order status atom: `<status-obs> <op> <other>` OR
/// `<other> <op> <status-obs>` — ONE destructure shared by the
/// evaluator and the literal collector.
fn status_atom<'e>(
    lhs: &'e Expr,
    rhs: &'e Expr,
    bindings: &'e StatusBindings,
) -> Option<(&'e str, &'e Expr)> {
    match (status_ref(lhs, bindings), status_ref(rhs, bindings)) {
        (Some(id), None) => Some((id, rhs)),
        (None, Some(id)) => Some((id, lhs)),
        _ => None,
    }
}

/// Evaluate a gate under one status assignment — Kleene-3, total.
/// `sigma` maps task ids to their assigned status bit.
fn eval_k3<'e>(e: &'e Expr, sigma: &BTreeMap<&str, u8>, b: &'e StatusBindings) -> K3 {
    match e {
        Expr::Lit(Literal::Bool(v)) => {
            if *v {
                K3::True
            } else {
                K3::False
            }
        }
        Expr::Not(inner) => eval_k3(inner, sigma, b).negate(),
        Expr::And(x, y) => eval_k3(x, sigma, b).and(eval_k3(y, sigma, b)),
        Expr::Or(x, y) => eval_k3(x, sigma, b).or(eval_k3(y, sigma, b)),
        Expr::Relation { op, lhs, rhs } => eval_relation(*op, lhs, rhs, sigma, b),
        // ternary/has/size/strings/members/… — beyond the status
        // fragment: Unknown (sound — never contributes to a dead-claim)
        _ => K3::Unknown,
    }
}

/// A relation — exact over status atoms, Unknown beyond them.
fn eval_relation<'e>(
    op: RelOp,
    lhs: &'e Expr,
    rhs: &'e Expr,
    sigma: &BTreeMap<&str, u8>,
    b: &'e StatusBindings,
) -> K3 {
    let Some((id, other)) = status_atom(lhs, rhs, b) else {
        return K3::Unknown;
    };
    let Some(&assigned) = sigma.get(id) else {
        return K3::Unknown;
    };
    match (op, other) {
        (RelOp::Eq | RelOp::Ne, Expr::Lit(Literal::Str(lit))) => {
            // an out-of-vocabulary literal NEVER equals a real status
            let holds = status_bit(lit).is_some_and(|bit| bit == assigned);
            let holds = if op == RelOp::Eq { holds } else { !holds };
            if holds { K3::True } else { K3::False }
        }
        (RelOp::In, Expr::List(items)) => {
            let holds = items.iter().any(|item| {
                matches!(item, Expr::Lit(Literal::Str(lit))
                    if status_bit(lit).is_some_and(|bit| bit == assigned))
            });
            if holds { K3::True } else { K3::False }
        }
        _ => K3::Unknown,
    }
}

/// Walk every sub-expression (the one recursion both collectors share).
fn walk<'e>(e: &'e Expr, visit: &mut dyn FnMut(&'e Expr)) {
    visit(e);
    match e {
        Expr::Not(a) | Expr::SizeCall(a) | Expr::HasCall(a) | Expr::SizeMethod(a) => {
            walk(a, visit);
        }
        Expr::And(a, b) | Expr::Or(a, b) => {
            walk(a, visit);
            walk(b, visit);
        }
        Expr::Relation { lhs, rhs, .. } => {
            walk(lhs, visit);
            walk(rhs, visit);
        }
        Expr::Ternary { cond, then, else_ } => {
            walk(cond, visit);
            walk(then, visit);
            walk(else_, visit);
        }
        Expr::StringMethod { base, arg, .. } => {
            walk(base, visit);
            walk(arg, visit);
        }
        Expr::Member { base, .. } => walk(base, visit),
        Expr::Index { base, index } => {
            walk(base, visit);
            walk(index, visit);
        }
        Expr::List(items) => {
            for item in items {
                walk(item, visit);
            }
        }
        Expr::Ident(_) | Expr::Lit(_) => {}
    }
}

/// Collect the distinct tasks referenced by status atoms.
fn collect_status_refs<'e>(e: &'e Expr, b: &'e StatusBindings) -> Vec<&'e str> {
    let mut out = Vec::new();
    // Set-based dedup keeps this O(n log n): the `when:` gate is
    // attacker-authored and a linear `Vec::contains` scan per atom is
    // quadratic on a hostile expression (see `collect_bad_literals`).
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    walk(e, &mut |node| {
        if let Some(id) = status_ref(node, b)
            && seen.insert(id)
        {
            out.push(id);
        }
    });
    out
}

/// The longest `Expr::List` anywhere in the gate expression — one O(n) pass,
/// the back-off guard for [`judge_gate`] (see [`MAX_GATE_LIST_ITEMS`]).
fn max_list_len(e: &Expr) -> usize {
    let mut max = 0;
    walk(e, &mut |node| {
        if let Expr::List(items) = node {
            max = max.max(items.len());
        }
    });
    max
}

/// Collect out-of-vocabulary status literals (the `'failed'` class).
fn collect_bad_literals(e: &Expr, b: &StatusBindings) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    // The `when:` gate is attacker-authored and an `in [...]` list literal
    // is uncapped (each element is depth-1, so MAX_DEPTH never fires). A
    // linear `Vec::contains` dedup made this O(n²) — a 40k-literal list of
    // distinct non-statuses burned ~3s of CPU on a 2-task workflow,
    // bypassing every cap (MAX_TASKS · MAX_DEPTH · MAX_GATE_REFS). The
    // `seen` set keeps dedup O(log n) per push → O(n log n) overall.
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    walk(e, &mut |node| {
        let Expr::Relation { op, lhs, rhs } = node else {
            return;
        };
        let Some((id, other)) = status_atom(lhs, rhs, b) else {
            return;
        };
        let mut push = |lit: &str| {
            let pair = (id.to_owned(), lit.to_owned());
            if seen.insert(pair.clone()) {
                out.push(pair);
            }
        };
        match (op, other) {
            (RelOp::Eq | RelOp::Ne, Expr::Lit(Literal::Str(lit))) => {
                if status_bit(lit).is_none() {
                    push(lit);
                }
            }
            (RelOp::In, Expr::List(items)) => {
                for item in items {
                    if let Expr::Lit(Literal::Str(lit)) = item
                        && status_bit(lit).is_none()
                    {
                        push(lit);
                    }
                }
            }
            _ => {}
        }
    });
    out
}

/// The per-gate verdict over the upstream possible-status sets.
struct GateVerdict {
    satisfiable: bool,
    falsifiable: bool,
}

/// Enumerate every assignment of the referenced tasks' possible sets.
fn judge_gate<'e>(
    expr: &'e Expr,
    possible: &BTreeMap<&str, u8>,
    b: &'e StatusBindings,
) -> GateVerdict {
    // A pathologically large `in [...]` list is not enumerated: each leaf
    // would re-scan it (O(4096 × len)). Widen to satisfiable∧falsifiable —
    // the sound back-off (no dead/redundant claim on an un-enumerated gate).
    if max_list_len(expr) > MAX_GATE_LIST_ITEMS {
        return GateVerdict {
            satisfiable: true,
            falsifiable: true,
        };
    }
    let refs = collect_status_refs(expr, b);
    // unknown refs (not yet computed / not a dep) widen to the full set
    let domains: Vec<(&str, u8)> = refs
        .iter()
        .map(|id| (*id, possible.get(id).copied().unwrap_or(S_ALL)))
        .collect();
    if domains.len() > MAX_GATE_REFS {
        return GateVerdict {
            satisfiable: true,
            falsifiable: true,
        };
    }
    let mut verdict = GateVerdict {
        satisfiable: false,
        falsifiable: false,
    };
    let mut sigma: BTreeMap<&str, u8> = BTreeMap::new();
    enumerate(expr, &domains, 0, &mut sigma, &mut verdict, b);
    verdict
}

/// Depth-first product walk — stops once both flags are set.
fn enumerate<'d>(
    expr: &Expr,
    domains: &'d [(&'d str, u8)],
    depth: usize,
    sigma: &mut BTreeMap<&'d str, u8>,
    verdict: &mut GateVerdict,
    b: &'d StatusBindings,
) {
    if verdict.satisfiable && verdict.falsifiable {
        return;
    }
    let Some((id, set)) = domains.get(depth) else {
        match eval_k3(expr, sigma, b) {
            K3::True => verdict.satisfiable = true,
            K3::False => verdict.falsifiable = true,
            // Unknown: could go either way at runtime — both
            K3::Unknown => {
                verdict.satisfiable = true;
                verdict.falsifiable = true;
            }
        }
        return;
    };
    for bit in [S_SUCCESS, S_FAILURE, S_SKIPPED, S_CANCELLED] {
        if set & bit != 0 {
            sigma.insert(id, bit);
            enumerate(expr, domains, depth + 1, sigma, verdict, b);
        }
    }
    sigma.remove(id);
}

/// Run the reachability analysis. `waves` is the valid topological
/// order (the caller — `check()` — only invokes this when conformance
/// holds, the same gating as the IFC secret analysis).
pub(crate) fn scan_gates(
    wf: &RawWorkflow,
    waves: &[Vec<usize>],
    edges: &[crate::analyzer::edges::Edge],
) -> Vec<GateFinding> {
    let mut findings = Vec::new();
    // possible terminal-status set per task, keyed by id
    let mut possible: BTreeMap<&str, u8> = BTreeMap::new();

    // The Graph IR — the abstract gate judges the SAME edges the
    // runtime will (one-truth: the analyzer derived them ONCE, this
    // pass consumes them — a re-derivation here cost check ~2× on the
    // bench and split the truth in two).
    let mut incoming: Vec<Vec<(usize, crate::analyzer::edges::EdgeKind)>> =
        vec![Vec::new(); wf.tasks.len()];
    for e in edges {
        incoming[e.to].push((e.from, e.kind));
    }

    for wave in waves {
        for &idx in wave {
            let task = &wf.tasks[idx].value;
            let id = task.id.value.as_str();
            let bindings = status_bindings(task);
            let set = fold_task(
                task,
                id,
                &possible,
                &incoming[idx],
                wf,
                &bindings,
                &mut findings,
            );
            possible.insert(id, set);
        }
    }
    findings
}

/// The with-bindings of a task that carry EXACTLY one bare
/// `${{ tasks.<id>.status }}` island — the W2 observation idiom the
/// gate analysis resolves through (`with.<name> == 'success'`).
fn status_bindings(task: &nika_schema::raw::RawTask) -> StatusBindings {
    let mut out = StatusBindings::new();
    for (key, value) in &task.with {
        let serde_json::Value::String(s) = &value.value else {
            continue;
        };
        let t = s.trim();
        if !(t.starts_with("${{") && t.ends_with("}}")) {
            continue;
        }
        let Ok(islands) = scan_templates(t) else {
            continue;
        };
        let [island] = islands.as_slice() else {
            continue;
        };
        // the DIRECT form only — the binding IS the observation
        let Expr::Member { base, field } = &island.expr else {
            continue;
        };
        if field != "status" {
            continue;
        }
        let target = match base.as_ref() {
            Expr::Member { base, field } => match base.as_ref() {
                Expr::Ident(root) if root == "tasks" => Some(field.clone()),
                _ => None,
            },
            Expr::Index { base, index } => match (base.as_ref(), index.as_ref()) {
                (Expr::Ident(root), Expr::Lit(Literal::Str(id))) if root == "tasks" => {
                    Some(id.clone())
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(target) = target {
            out.insert(key.value.clone(), target);
        }
    }
    out
}

/// The bit-mask of settled states an edge admits (GATE-v2 pass-sets ·
/// `analyzer::edges::EdgeKind::admits` projected onto the bitset).
fn edge_mask(kind: crate::analyzer::edges::EdgeKind) -> u8 {
    use crate::analyzer::edges::SettledState;
    let mut mask = 0;
    for (state, bit) in [
        (SettledState::Success, S_SUCCESS),
        (SettledState::Failure, S_FAILURE),
        (SettledState::Skipped, S_SKIPPED),
        (SettledState::Cancelled, S_CANCELLED),
    ] {
        if kind.admits(state) {
            mask |= bit;
        }
    }
    mask
}

/// One task's possible terminal-status set — pushing findings along
/// the way (GATE-v2 · the gate is judged over the DERIVED edges, then
/// `when:` refines POST-gate · spec 03 §gate algebra).
#[allow(clippy::too_many_arguments)]
fn fold_task(
    task: &nika_schema::raw::RawTask,
    id: &str,
    possible: &BTreeMap<&str, u8>,
    incoming: &[(usize, crate::analyzer::edges::EdgeKind)],
    wf: &RawWorkflow,
    bindings: &StatusBindings,
    findings: &mut Vec<GateFinding>,
) -> u8 {
    let span = task
        .when
        .as_ref()
        .map(|w| ByteSpan::new(w.span.start.0, w.span.end.0));

    // `on_error: { skip: true }` makes `skipped` reachable
    let skip_route = task
        .on_error
        .as_ref()
        .is_some_and(|oe| matches!(oe.value.action, OnErrorAction::Skip));

    let mut set = S_CANCELLED; // always possible (operator · timeout · budget)

    let gate_live = gate_can_admit(incoming, possible, wf, id, span, findings);

    if gate_live {
        match task.when.as_ref().map(|w| &w.value) {
            Some(WhenGate::Literal(false)) => {
                // the documented never-pattern (feature-flag) — not a
                // finding; the task is skipped by explicit intent
                // (POST-gate · skipped, never cancelled)
                set |= S_SKIPPED;
            }
            Some(WhenGate::Literal(true)) | None => {
                set |= S_SUCCESS | S_FAILURE;
            }
            Some(WhenGate::Expr(src)) => match parse_gate(src) {
                Some(expr) => {
                    for (ref_task, lit) in collect_bad_literals(&expr, bindings) {
                        let fix = closest_status(&lit).map(|s| format!("did you mean '{s}'?"));
                        findings.push(GateFinding {
                            task: id.to_owned(),
                            kind: GateFindingKind::BadStatusLiteral,
                            detail: format!(
                                "the status of `{ref_task}` is compared against '{lit}' — \
                                 not a status (the vocabulary is success · failure · \
                                 skipped · cancelled), so `==` never matches and `!=` \
                                 always holds"
                            ),
                            fix,
                            span,
                        });
                    }
                    let verdict = judge_gate(&expr, possible, bindings);
                    if verdict.satisfiable {
                        set |= S_SUCCESS | S_FAILURE;
                    } else {
                        findings.push(GateFinding {
                            task: id.to_owned(),
                            kind: GateFindingKind::DeadTask,
                            detail: dead_detail(&expr, possible, bindings),
                            fix: None,
                            span,
                        });
                    }
                    if verdict.falsifiable {
                        set |= S_SKIPPED;
                    }
                }
                // unparseable when: the analyzer owns that error — stay
                // silent and assume anything
                None => set |= S_SUCCESS | S_FAILURE | S_SKIPPED,
            },
        }
        // an empty for_each collection settles `skipped` (spec 03
        // §for_each semantics) — the state is reachable whenever the
        // task can run at all.
        if task.for_each.is_some() {
            set |= S_SKIPPED;
        }
    }
    if skip_route {
        set |= S_SKIPPED;
    }
    set
}

/// GATE-v2 admission liveness — every incoming edge must be ABLE to
/// admit: a producer whose whole possible-set lies outside an edge's
/// pass-set makes the consumer permanently dead (cancelled on every
/// run · a `DeadTask` finding per such edge).
fn gate_can_admit(
    incoming: &[(usize, crate::analyzer::edges::EdgeKind)],
    possible: &BTreeMap<&str, u8>,
    wf: &RawWorkflow,
    id: &str,
    span: Option<ByteSpan>,
    findings: &mut Vec<GateFinding>,
) -> bool {
    let mut gate_live = true;
    for &(from, kind) in incoming {
        let producer = wf.tasks[from].value.id.value.as_str();
        let producer_possible = possible.get(producer).copied().unwrap_or(S_ALL);
        if producer_possible & edge_mask(kind) == 0 {
            gate_live = false;
            findings.push(GateFinding {
                task: id.to_owned(),
                kind: GateFindingKind::DeadTask,
                detail: format!(
                    "the {} edge from `{producer}` can never admit — `{producer}` \
                     can only settle a state outside the edge's pass-set (spec 03 \
                     \u{a7}gate algebra v2); this task is cancelled on every run",
                    kind.wire_kind()
                ),
                fix: None,
                span,
            });
        }
    }
    gate_live
}

/// The diagnostic for a dead gate — names the upstream sets so the
/// author sees WHY (the « with diagnostics » discipline).
fn dead_detail<'e>(expr: &'e Expr, possible: &BTreeMap<&str, u8>, b: &'e StatusBindings) -> String {
    let mut parts: Vec<String> = Vec::new();
    for id in collect_status_refs(expr, b) {
        let set = possible.get(id).copied().unwrap_or(S_ALL);
        let names: Vec<&str> = [
            (S_SUCCESS, "success"),
            (S_FAILURE, "failure"),
            (S_SKIPPED, "skipped"),
            (S_CANCELLED, "cancelled"),
        ]
        .iter()
        .filter(|(bit, _)| set & bit != 0)
        .map(|(_, n)| *n)
        .collect();
        parts.push(format!("`{id}` can only be {{{}}}", names.join(", ")));
    }
    let upstream = if parts.is_empty() {
        String::new()
    } else {
        format!(" — upstream: {}", parts.join(" · "))
    };
    format!(
        "the `when:` gate is FALSE under every reachable combination of upstream \
         statuses — this task can never run{upstream}"
    )
}

/// Parse the single boolean island of a `when:` gate.
pub(crate) fn parse_gate(src: &str) -> Option<Expr> {
    let islands = scan_templates(src).ok()?;
    let island = islands.into_iter().next()?;
    Some(island.expr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::{FileId, ParseMode, parse};

    fn gates(yaml: &str) -> Vec<GateFinding> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let analyzed = crate::analyze(&wf).expect("analyze");
        scan_gates(&wf, &analyzed.topo_waves, &analyzed.edges)
    }

    fn wf(tasks: &str) -> String {
        format!("nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n{tasks}")
    }

    #[test]
    fn contradiction_on_one_task_is_dead() {
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status == 'success' && with.a_status == 'failure' }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].kind, GateFindingKind::DeadTask);
        assert_eq!(f[0].task, "b");
        assert!(f[0].detail.contains("can never run"), "{}", f[0].detail);
        assert!(f[0].span.is_some(), "the gate carries its span");
    }

    #[test]
    fn oversized_gate_in_list_is_bounded_not_quadratic() {
        // Gate-11 F2 regression (2026-06-16): a `when:` gate's `in [...]` list
        // was re-scanned by each of the 4^MAX_GATE_REFS leaf evaluations →
        // O(4096 × len) (a 3.6 MiB gate ≈ 0.9 s). `max_list_len` + the
        // `judge_gate` back-off bound it to one O(n) pass. A list past
        // `MAX_GATE_LIST_ITEMS` settles instantly; the gate stays live (the
        // back-off widens to satisfiable∧falsifiable — never a false finding).
        use std::fmt::Write as _;
        let mut list = String::from("'success'");
        for _ in 0..5_000 {
            write!(list, ", 'success'").expect("write to String is infallible");
        }
        let f = gates(&wf(&format!(
            "  a:\n    exec: {{ command: [\"true\"] }}\n  b:\n    with: {{ a_status: \"${{{{ tasks.a.status }}}}\" }}\n    when: ${{{{ with.a_status in [{list}] }}}}\n    exec: {{ command: [\"true\"] }}\n"
        )));
        assert!(
            !f.iter().any(|g| g.task == "b"),
            "an oversized-but-live gate must not be flagged: {f:?}"
        );
    }

    #[test]
    fn bad_status_literal_failed_is_caught_with_the_fix() {
        // the wild-caught class: 'failed' is not a status — 'failure' is
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status == 'failed' }}\n    exec: { command: [\"true\"] }\n",
        ));
        // == 'failed' never matches → ALSO dead
        assert_eq!(f.len(), 2, "{f:?}");
        assert_eq!(f[0].kind, GateFindingKind::BadStatusLiteral);
        assert_eq!(f[0].fix.as_deref(), Some("did you mean 'failure'?"));
        assert_eq!(f[1].kind, GateFindingKind::DeadTask);
    }

    #[test]
    fn ne_bad_literal_flags_vocabulary_but_lives() {
        // != 'failed' always holds — a bug, but the task CAN run
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status != 'failed' }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].kind, GateFindingKind::BadStatusLiteral);
    }

    #[test]
    fn impossible_skipped_without_a_skip_route_is_dead() {
        // `a` has no when: and no on_error:skip — it can never be
        // `skipped`, so gating b on it is dead (the spec's own note)
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status == 'skipped' }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].kind, GateFindingKind::DeadTask);
        assert!(f[0].detail.contains("`a` can only be"), "{}", f[0].detail);
    }

    #[test]
    fn skip_route_makes_skipped_reachable() {
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n    on_error: { skip: true }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status == 'skipped' }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn failure_gate_is_alive_and_cancelled_gate_is_alive() {
        // == 'failure' is the documented escalation pattern; cancelled
        // is always possible (operator stop) — neither is dead (the
        // status binding is a terminal-observation edge · admits all)
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status == 'failure' }}\n    exec: { command: [\"true\"] }\n  c:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status == 'cancelled' }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn dead_cascades_through_the_dag() {
        // b is dead (contradiction) → b can only be skipped/cancelled →
        // c gated on b == 'success' is dead TOO, with the diagnostic
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status == 'success' && with.a_status == 'failure' }}\n    exec: { command: [\"true\"] }\n  c:\n    with: { b_status: \"${{ tasks.b.status }}\" }\n    when: ${{ with.b_status == 'success' }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert_eq!(f.len(), 2, "{f:?}");
        assert!(f.iter().all(|g| g.kind == GateFindingKind::DeadTask));
        let c = f.iter().find(|g| g.task == "c").expect("c dead");
        assert!(
            c.detail.contains("`b` can only be {skipped, cancelled}"),
            "{}",
            c.detail
        );
    }

    #[test]
    fn unknown_atoms_are_sound_not_dead() {
        // vars/env/output atoms → Unknown → never a dead-claim
        let f = gates(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\nconst: { env: \"staging\" }\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ const.env == 'production' && with.a_status == 'success' }}\n    exec: { command: [\"true\"] }\n  c:\n    with:\n      a_out_x: \"${{ tasks.a.output.x }}\"\n      a_status: \"${{ tasks.a.status }}\"\n    when: \"${{ has(with.a_out_x) ? with.a_status == 'success' : false }}\"\n    exec: { command: [\"true\"] }\n",
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn in_list_with_vocab_lives_and_bad_member_is_flagged() {
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status in ['success', 'skipped'] }}\n    exec: { command: [\"true\"] }\n  c:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status in ['failed'] }}\n    exec: { command: [\"true\"] }\n",
        ));
        // b: alive (in-list over vocab — skipped unreachable for a, but
        // success IS reachable → satisfiable). c: bad literal AND dead.
        assert_eq!(f.len(), 2, "{f:?}");
        assert!(
            f.iter()
                .any(|g| g.task == "c" && g.kind == GateFindingKind::BadStatusLiteral)
        );
        assert!(
            f.iter()
                .any(|g| g.task == "c" && g.kind == GateFindingKind::DeadTask)
        );
    }

    #[test]
    fn when_false_literal_is_the_documented_never_pattern_not_a_finding() {
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    when: false\n    exec: { command: [\"true\"] }\n",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn k3_truth_tables_are_kleene() {
        use K3::{False as F, True as T, Unknown as U};
        // and — strong Kleene
        let and_table = [
            (T, T, T),
            (T, F, F),
            (F, T, F),
            (F, F, F),
            (T, U, U),
            (U, T, U),
            (F, U, F),
            (U, F, F),
            (U, U, U),
        ];
        for (a, b, want) in and_table {
            assert_eq!(a.and(b), want, "{a:?} AND {b:?}");
        }
        // or — the dual
        let or_table = [
            (T, T, T),
            (T, F, T),
            (F, T, T),
            (F, F, F),
            (T, U, T),
            (U, T, T),
            (F, U, U),
            (U, F, U),
            (U, U, U),
        ];
        for (a, b, want) in or_table {
            assert_eq!(a.or(b), want, "{a:?} OR {b:?}");
        }
        assert_eq!(T.negate(), F);
        assert_eq!(F.negate(), T);
        assert_eq!(U.negate(), U);
    }

    #[test]
    fn status_bits_are_disjoint_and_all_covers_exactly_four() {
        let bits = [S_SUCCESS, S_FAILURE, S_SKIPPED, S_CANCELLED];
        for (i, a) in bits.iter().enumerate() {
            for b in &bits[i + 1..] {
                assert_eq!(a & b, 0, "bits must be disjoint");
            }
        }
        assert_eq!(S_ALL, S_SUCCESS + S_FAILURE + S_SKIPPED + S_CANCELLED);
        assert_eq!(S_ALL.count_ones(), 4);
    }

    #[test]
    fn index_form_status_ref_is_the_same_atom() {
        // the index forms must hit the same analysis — `tasks['a'].status`
        // on the binding side, `with['a_status']` on the gate side:
        // alive on 'failure', dead on impossible 'skipped'
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks['a'].status }}\" }\n    when: ${{ with.a_status == 'failure' }}\n    exec: { command: [\"true\"] }\n  c:\n    with: { a_status: \"${{ tasks['a'].status }}\" }\n    when: ${{ with['a_status'] == 'skipped' }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].task, "c");
        assert_eq!(f[0].kind, GateFindingKind::DeadTask);
    }

    #[test]
    fn reversed_operand_order_is_the_same_atom() {
        // 'skipped' == with.a_status — literal first, same verdict
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ 'skipped' == with.a_status }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].kind, GateFindingKind::DeadTask);
    }

    #[test]
    fn when_false_makes_skipped_reachable_downstream() {
        // b is when:false (skipped by intent — S_SKIPPED reachable) —
        // c gated on b == 'skipped' is ALIVE: the never-pattern makes
        // skipped a real status downstream
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    when: false\n    exec: { command: [\"true\"] }\n  c:\n    with: { b_status: \"${{ tasks.b.status }}\" }\n    when: ${{ with.b_status == 'skipped' }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn status_ref_requires_the_tasks_root_exactly() {
        // direct AST: vars.a.status / item.a.status are NOT status atoms
        let none = StatusBindings::new();
        let mk = |root: &str| Expr::Member {
            base: Box::new(Expr::Member {
                base: Box::new(Expr::Ident(root.to_owned())),
                field: "a".to_owned(),
            }),
            field: "status".to_owned(),
        };
        assert_eq!(status_ref(&mk("tasks"), &none), Some("a"));
        assert_eq!(status_ref(&mk("vars"), &none), None);
        assert_eq!(status_ref(&mk("item"), &none), None);
        // index form: tasks['a'].status vs vars['a'].status
        let mk_idx = |root: &str| Expr::Member {
            base: Box::new(Expr::Index {
                base: Box::new(Expr::Ident(root.to_owned())),
                index: Box::new(Expr::Lit(Literal::Str("a".to_owned()))),
            }),
            field: "status".to_owned(),
        };
        assert_eq!(status_ref(&mk_idx("tasks"), &none), Some("a"));
        assert_eq!(status_ref(&mk_idx("vars"), &none), None);
    }

    #[test]
    fn status_ref_resolves_with_bindings() {
        // W2: `with.st` IS a status atom when the binding `st` carries
        // exactly one bare `${{ tasks.a.status }}` island — member and
        // index forms resolve through the same map; an unknown binding
        // (or an empty map) resolves nothing.
        let mut b = StatusBindings::new();
        b.insert("st".to_owned(), "a".to_owned());
        let member = Expr::Member {
            base: Box::new(Expr::Ident("with".to_owned())),
            field: "st".to_owned(),
        };
        assert_eq!(status_ref(&member, &b), Some("a"));
        let index = Expr::Index {
            base: Box::new(Expr::Ident("with".to_owned())),
            index: Box::new(Expr::Lit(Literal::Str("st".to_owned()))),
        };
        assert_eq!(status_ref(&index, &b), Some("a"));
        let unknown = Expr::Member {
            base: Box::new(Expr::Ident("with".to_owned())),
            field: "other".to_owned(),
        };
        assert_eq!(status_ref(&unknown, &b), None);
        assert_eq!(status_ref(&member, &StatusBindings::new()), None);
    }

    #[test]
    fn eval_k3_handles_the_boolean_arms_exactly() {
        let sigma: BTreeMap<&str, u8> = BTreeMap::new();
        let none = StatusBindings::new();
        let t = Expr::Lit(Literal::Bool(true));
        let f = Expr::Lit(Literal::Bool(false));
        // Lit(Bool) is exact — not Unknown
        assert_eq!(eval_k3(&t, &sigma, &none), K3::True);
        assert_eq!(eval_k3(&f, &sigma, &none), K3::False);
        // Not / Or are exact over exact operands
        assert_eq!(
            eval_k3(&Expr::Not(Box::new(f.clone())), &sigma, &none),
            K3::True
        );
        assert_eq!(
            eval_k3(
                &Expr::Or(Box::new(f.clone()), Box::new(t.clone())),
                &sigma,
                &none
            ),
            K3::True
        );
        assert_eq!(
            eval_k3(&Expr::And(Box::new(t), Box::new(f)), &sigma, &none),
            K3::False
        );
    }

    #[test]
    fn reversed_operand_bad_literal_is_flagged_too() {
        // 'failed' == with.a_status — literal first, vocabulary still checked
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ 'failed' == with.a_status }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert!(
            f.iter()
                .any(|g| g.kind == GateFindingKind::BadStatusLiteral),
            "{f:?}"
        );
    }

    #[test]
    fn the_ref_cap_boundary_still_enumerates_at_exactly_six() {
        // 6 distinct resolved refs = the cap boundary — MUST still
        // enumerate (a contradiction among them is provably dead); 7+
        // refs back off to satisfiable. The mutant `> 6` → `>= 6`/`== 6`
        // dies here. u1 is referenced twice through its ONE binding —
        // dedup is by resolved task, exactly as pre-W2.
        use std::fmt::Write as _;
        let mut tasks = String::new();
        for i in 1..=6 {
            let _ = write!(tasks, "  u{i}:\n    exec: {{ command: [\"true\"] }}\n");
        }
        tasks.push_str("  z:\n    with:\n");
        for i in 1..=6 {
            let _ = writeln!(tasks, "      u{i}_status: \"${{{{ tasks.u{i}.status }}}}\"");
        }
        tasks.push_str(
            "    when: ${{ with.u1_status == 'success' && with.u1_status == 'failure' && with.u2_status == 'success' && with.u3_status == 'success' && with.u4_status == 'success' && with.u5_status == 'success' && with.u6_status == 'success' }}\n    exec: { command: [\"true\"] }\n",
        );
        let f = gates(&wf(&tasks));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].kind, GateFindingKind::DeadTask);
    }

    #[test]
    fn satisfiable_chains_keep_success_reachable() {
        // alive chain through BOTH gate forms: when:true and a sat expr —
        // the |= S_SUCCESS paths must really add SUCCESS (a &= mutant
        // kills the chain and the downstream gates go dead)
        let f = gates(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    when: true\n    exec: { command: [\"true\"] }\n  c:\n    with: { b_status: \"${{ tasks.b.status }}\" }\n    when: ${{ with.b_status == 'success' }}\n    exec: { command: [\"true\"] }\n  d:\n    with: { c_status: \"${{ tasks.c.status }}\" }\n    when: ${{ with.c_status == 'success' }}\n    exec: { command: [\"true\"] }\n",
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn vocabulary_is_pinned_to_the_spec() {
        // spec 03-dag.md §Task states — the four when:-observable
        // terminal statuses, verbatim
        assert_eq!(STATUS_VOCAB, ["success", "failure", "skipped", "cancelled"]);
    }

    #[test]
    fn bad_literals_dedup_repeated_members() {
        // Repeated identical out-of-vocab members collapse to ONE pair —
        // dedup is by (ref_task, literal), not by position.
        let expr =
            parse_gate("${{ tasks.a.status in ['nope', 'nope', 'nope'] }}").expect("gate parses");
        assert_eq!(collect_bad_literals(&expr, &StatusBindings::new()).len(), 1);
    }

    #[test]
    fn huge_in_list_of_distinct_literals_is_not_quadratic() {
        // DoS regression guard: an `in [...]` list literal is uncapped (each
        // element is depth-1, so MAX_DEPTH never fires) and feeds
        // collect_bad_literals. With the old `Vec::contains` dedup, 20k
        // distinct non-status literals were O(n²) — seconds of CPU on a
        // 2-task workflow, bypassing MAX_TASKS / MAX_DEPTH / MAX_GATE_REFS.
        // With set-based dedup it is O(n log n): this test COMPLETING fast
        // is the proof. All 20k distinct literals are still collected.
        let n = 20_000usize;
        let list = (0..n)
            .map(|i| format!("'x{i}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let src = format!("${{{{ tasks.a.status in [{list}] }}}}");
        let expr = parse_gate(&src).expect("gate parses");
        assert_eq!(collect_bad_literals(&expr, &StatusBindings::new()).len(), n);
    }

    #[test]
    fn default_gate_with_deps_keeps_downstream_success_reachable() {
        // A task with incoming edges and no `when:` must keep SUCCESS
        // reachable when its gate is live. `b` here has a control edge
        // from `a` and no gate; `c` observes `b` and gates on success.
        // If the gate-live fold wrongly empties b's set (a `|=` →
        // `&=` mutant on the S_SUCCESS path, or an inverted edge-mask
        // test), `b` stays {cancelled} → `c`'s success gate goes DEAD.
        // Asserting `c` is alive pins the default-gate path through a
        // downstream status observation.
        let f = gates(&wf("  a:\n    exec: { command: [\"true\"] }\n  b:\n    \
             after: { a: success }\n    exec: { command: [\"true\"] }\n  c:\n    \
             with: { b_status: \"${{ tasks.b.status }}\" }\n    when: ${{ with.b_status == 'success' }}\n    \
             exec: { command: [\"true\"] }\n"));
        assert!(f.is_empty(), "{f:?}");
    }
}
