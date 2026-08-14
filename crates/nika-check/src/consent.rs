// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The affirmative-consent lane (NEP-0020 · `NIKA-SEC-014` · P0-2 of the
//! 2026-07-30 UX audit — the hint-only lane landed 2026-07-30 and
//! escalates here to the refusal it was measuring).
//!
//! The defect: a REFUSED confirm is a task SUCCESS whose value is
//! `false` — the runtime records the Deny in the approval attestation
//! only (`nika-runtime/src/approval.rs` · `SettleAs::Ran(Success)`), so
//! every route that does not consume the answer passes the effect
//! through:
//!
//! - a bare state edge `after: { ask: success }` — the refusal settles
//!   `success`, the edge admits it, the exec fires;
//! - a `when:` that never references the prompt's output — the answer
//!   is decoration;
//! - a `when:` that references the output but stays TRUE on `false`
//!   (`with.go == true || with.go == false`) — the refusal cannot block.
//!
//! The law (spec `10-authority.md` §the affirmative-consent law):
//! **false triggers exactly zero effects.** For every confirm-mode
//! `invoke: nika:prompt` and every egress-capable task (the ONE effect
//! table, `trifecta::egress_capable`), every route from the gate to the
//! task must be CLOSED — by an affirmative gate (its `when:` evaluates
//! to [`K3::False`] under the refusal substitution), by `when: false`,
//! or by a closer confirm gate (the nearest gate owns its closure — the
//! approval-batch precedent).
//!
//! **Sound, never a false red.** The blocking row fires only on the
//! PROVEN route: every edge on it admits the refusal (the pass-set —
//! a refusal settles `Success`, a `failure`/`skipped` predicate carries
//! nothing) and every gate on it is proven open. A gate the fragment
//! cannot decide (a nested template binding carrying the answer · a
//! non-fragment expression) makes the route UNPROVEN — the advisory
//! hint's ground, exactly the pre-escalation behavior, never a code.
//! `mode: choice` stays out of scope (silence, never wrong).
//!
//! Method: per confirm-mode prompt, a BFS over the refusal-admitting
//! derived edges carrying the route's proof state (clean vs tainted —
//! a sink reached BOTH ways refuses, the proven route dominates). The
//! refusal substitution resolves the gate's settled facts: the exact
//! single-island `with:` carrier of `tasks.<prompt>.output` is `false`,
//! the one of `tasks.<prompt>.status` is `"success"` (a status read is
//! decidable, and it is NOT consent). The lane is gated on a valid DAG
//! (the caller's IFC/policy gating) and skips past the analysis task
//! cap (the O(P·E) per-prompt walk shares the `DoS` floor).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nika_schema::expression::{Expr, Literal, NamespaceRef, RelOp, expr_refs, scan_templates};
use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use nika_schema::types::WhenGate;

use crate::analyzer::{Edge, SettledState};
use crate::hints::Hint;
use crate::reach::K3;

/// The blocking row (NEP-0020 · `NIKA-SEC-014`): a confirm-mode human
/// gate whose refusal an egress-capable task cannot escape — the
/// witness names the gate AND the sink, the fix teaches the
/// affirmative pattern.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ConsentFinding {
    /// The confirm gate whose answer leaks (`tasks.<prompt>`).
    pub prompt: String,
    /// The egress-capable task the refusal reaches.
    pub sink: String,
    /// The human row — the defect, its mechanism, the repair.
    pub detail: String,
}

impl ConsentFinding {
    /// The ONE wire code (spec 10 · NEP-0020) — every surface reads it
    /// (the findings fold · the extra-conformance list).
    pub const WIRE_CODE: &'static str = "NIKA-SEC-014";
}

/// The lane's two outputs: the PROVEN refusal (blocking) and the
/// undecidable remainder (advisory — the pre-escalation hint).
#[derive(Debug, Default)]
pub(crate) struct ConsentScan {
    /// One row per (gate, sink) pair on a proven non-affirmative route.
    pub(crate) findings: Vec<ConsentFinding>,
    /// One row per (gate, sink-or-gate) pair the fragment cannot decide.
    pub(crate) hints: Vec<Hint>,
}

/// The task's `when:` under THIS prompt's refusal — the three fates
/// (spec 10 §the affirmative-consent law).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gate {
    /// Proven FALSE under the refusal (the affirmative gate ·
    /// `when: false`) — the route is closed.
    Closed,
    /// No `when:`, or a gate proven TRUE under the refusal — the
    /// refusal flows through.
    Open,
    /// The fragment cannot decide (a nested binding · a non-fragment
    /// expression) — the defect is unproven: advisory, never a refusal.
    Unclear,
}

/// Judge the affirmative-consent lane over the derived graph. Empty
/// unless a confirm-mode prompt reaches an egress-capable descendant
/// over a route that never gates on the answer.
pub(crate) fn scan_consent(wf: &RawWorkflow, edges: &[Edge]) -> ConsentScan {
    let mut scan = ConsentScan::default();
    if wf.tasks.len() > crate::analysis::ANALYSIS_TASK_CAP {
        return scan;
    }
    // Only the edges that ADMIT the refusal — it settles Success, so a
    // `failure`/`skipped`-only predicate carries nothing (the pass-set
    // is the soundness floor: a predicate-blind walk would red a
    // failure-edge route that can never fire).
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); wf.tasks.len()];
    for e in edges {
        if e.kind.admits(SettledState::Success) {
            children[e.from].push(e.to);
        }
    }
    let mut blocked: BTreeSet<(String, String)> = BTreeSet::new();
    let mut uncertain: BTreeSet<(String, String)> = BTreeSet::new();
    for (idx, task) in wf.tasks.iter().enumerate() {
        if !is_confirm_prompt(&task.value) {
            continue;
        }
        let prompt = task.value.id.value.as_str();
        // BFS with the route's proof state: `false` = every gate so far
        // is proven open (the refusal flows), `true` = an undecidable
        // gate taints the route. The clean state DOMINATES — a sink
        // reached both ways refuses (one proven route is enough).
        let mut best: BTreeMap<usize, bool> = BTreeMap::from([(idx, false)]);
        let mut queue: VecDeque<(usize, bool)> =
            children[idx].iter().map(|&c| (c, false)).collect();
        while let Some((n, tainted)) = queue.pop_front() {
            if best.get(&n).is_some_and(|&b| b <= tainted) {
                continue;
            }
            best.insert(n, tainted);
            let t = &wf.tasks[n].value;
            // A closer confirm gate owns its closure (the approval-batch
            // precedent); a gate proven FALSE under the refusal cuts the
            // route (the affirmative `when:` · `when: false`).
            if is_confirm_prompt(t) {
                continue;
            }
            match gate_verdict(t, prompt) {
                Gate::Closed => {}
                Gate::Open => {
                    if crate::trifecta::egress_capable(&t.action) {
                        if tainted {
                            uncertain.insert((prompt.to_owned(), t.id.value.clone()));
                        } else {
                            blocked.insert((prompt.to_owned(), t.id.value.clone()));
                        }
                    }
                    queue.extend(children[n].iter().map(|&c| (c, tainted)));
                }
                Gate::Unclear => {
                    if crate::trifecta::egress_capable(&t.action) {
                        uncertain.insert((prompt.to_owned(), t.id.value.clone()));
                    }
                    queue.extend(children[n].iter().map(|&c| (c, true)));
                }
            }
        }
    }
    // A sink that refuses on a proven route keeps no advisory twin.
    for (prompt, sink) in &blocked {
        scan.findings.push(consent_finding(prompt, sink));
    }
    for (prompt, sink) in &uncertain {
        if !blocked.contains(&(prompt.clone(), sink.clone())) {
            scan.hints.push(consent_hint(prompt, sink));
        }
    }
    scan
}

/// The task's `when:` under the refusal — see [`Gate`]. The gate
/// consumes the answer through the EXACT carriers the substitution
/// resolves; a carrier the fragment cannot resolve (a nested template ·
/// another field) makes any gate reading it unproven.
fn gate_verdict(task: &RawTask, prompt: &str) -> Gate {
    let Some(when) = task.when.as_ref() else {
        return Gate::Open;
    };
    let src = match &when.value {
        WhenGate::Literal(v) => return if *v { Gate::Open } else { Gate::Closed },
        WhenGate::Expr(src) => src,
    };
    let Some(expr) = crate::reach::parse_gate(src) else {
        return Gate::Unclear;
    };
    let env = RefusalEnv::of(task);
    let carrying = carrying_keys(task, prompt, &env);
    if expr_refs(&expr)
        .iter()
        .any(|r| matches!(r, NamespaceRef::With(k) if carrying.contains(k)))
    {
        return Gate::Unclear;
    }
    match eval_consent(&expr, prompt, &env) {
        K3::False => Gate::Closed,
        K3::True => Gate::Open,
        K3::Unknown => Gate::Unclear,
    }
}

/// A confirm-mode `invoke: nika:prompt` — `mode:` ABSENT is confirm
/// (the builtin's runtime default); another literal mode is a different
/// contract (choice answers are strings), a templated mode is not
/// judged (silence, never wrong).
fn is_confirm_prompt(task: &RawTask) -> bool {
    let RawAction::Invoke(inv) = &task.action else {
        return false;
    };
    let Some(tool) = inv.tool() else {
        return false;
    };
    if tool.value != nika_cap::HUMAN_GATE_TOOL {
        return false;
    }
    let mode = inv
        .args
        .as_ref()
        .and_then(|a| a.value.as_object())
        .and_then(|o| o.get("mode"))
        .and_then(serde_json::Value::as_str);
    mode.is_none_or(|m| m == "confirm")
}

/// The gate's settled facts under THIS prompt's refusal — the exact
/// single-island `with:` carriers the substitution resolves: the
/// `.output` carrier is `false`, the `.status` carrier is `"success"`
/// (a refusal settles success — a status read is decidable, and it is
/// NOT consent).
struct RefusalEnv {
    outputs: BTreeMap<String, String>,
    statuses: BTreeMap<String, String>,
}

impl RefusalEnv {
    /// Collect the task's exact carriers — only THIS prompt's are facts;
    /// the reads (`is_output_ref` / `is_status_ref`) match on the id.
    fn of(task: &RawTask) -> Self {
        let mut outputs = BTreeMap::new();
        let mut statuses = BTreeMap::new();
        for (key, value) in &task.with {
            let serde_json::Value::String(s) = &value.value else {
                continue;
            };
            if let Some(id) = exact_carrier(s, "output") {
                outputs.insert(key.value.clone(), id);
            }
            if let Some(id) = exact_carrier(s, "status") {
                statuses.insert(key.value.clone(), id);
            }
        }
        Self { outputs, statuses }
    }
}

/// The with-value IS exactly one bare `${{ tasks.<id>.<field> }}` island
/// — the W2 observation idiom (output flavor and status flavor share the
/// one shape).
fn exact_carrier(value: &str, field: &str) -> Option<String> {
    let t = value.trim();
    if !(t.starts_with("${{") && t.ends_with("}}")) {
        return None;
    }
    let Ok(islands) = scan_templates(t) else {
        return None;
    };
    let [island] = islands.as_slice() else {
        return None;
    };
    record_field(&island.expr, field)
}

/// The expr IS exactly `tasks.<id>.<field>` (member or index form).
fn record_field(e: &Expr, field: &str) -> Option<String> {
    let Expr::Member { base, field: f } = e else {
        return None;
    };
    if f != field {
        return None;
    }
    match base.as_ref() {
        Expr::Member { base, field } => match base.as_ref() {
            Expr::Ident(root) if root == "tasks" => Some(field.clone()),
            _ => None,
        },
        Expr::Index { base, index } => match (base.as_ref(), index.as_ref()) {
            (Expr::Ident(root), Expr::Lit(Literal::Str(id))) if root == "tasks" => Some(id.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// The with: keys carrying THIS prompt's record in a shape the
/// substitution cannot resolve (a nested template · a field other than
/// output/status) — a gate reading one is UNPROVEN, never affirmative.
fn carrying_keys(task: &RawTask, prompt: &str, env: &RefusalEnv) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (key, value) in &task.with {
        if env.outputs.get(&key.value).is_some_and(|id| id == prompt)
            || env.statuses.get(&key.value).is_some_and(|id| id == prompt)
        {
            continue;
        }
        let serde_json::Value::String(s) = &value.value else {
            continue;
        };
        let Ok(islands) = scan_templates(s) else {
            continue;
        };
        let carries = islands.iter().any(|i| {
            expr_refs(&i.expr)
                .iter()
                .any(|r| matches!(r, NamespaceRef::Tasks { id, .. } if id == prompt))
        });
        if carries {
            out.insert(key.value.clone());
        }
    }
    out
}

/// The gate sub-expression reads THIS prompt's answer — the direct
/// `tasks.<prompt>.output` form or a `with:` binding carrying it.
fn is_output_ref(e: &Expr, prompt: &str, env: &RefusalEnv) -> bool {
    if record_field(e, "output").as_deref() == Some(prompt) {
        return true;
    }
    with_ref_target(e, &env.outputs).is_some_and(|id| id == prompt)
}

/// The same read for the prompt's STATUS — decidable under the refusal
/// (it settles `"success"`), never consent.
fn is_status_ref(e: &Expr, prompt: &str, env: &RefusalEnv) -> bool {
    if record_field(e, "status").as_deref() == Some(prompt) {
        return true;
    }
    with_ref_target(e, &env.statuses).is_some_and(|id| id == prompt)
}

/// The carrier id a `with.<key>` reference resolves to, when the key is
/// an exact single-island binding.
fn with_ref_target<'a>(e: &Expr, b: &'a BTreeMap<String, String>) -> Option<&'a String> {
    match e {
        Expr::Member { base, field } => match base.as_ref() {
            Expr::Ident(root) if root == "with" => b.get(field),
            _ => None,
        },
        Expr::Index { base, index } => match (base.as_ref(), index.as_ref()) {
            (Expr::Ident(root), Expr::Lit(Literal::Str(name))) if root == "with" => b.get(name),
            _ => None,
        },
        _ => None,
    }
}

/// Kleene-3 evaluation of a `when:` gate with THIS prompt's settled
/// facts substituted (output = `false` · status = `"success"`) — exact
/// over the consent fragment (boolean literals · `==`/`!=`/`in` on
/// resolved literals · `!`/`&&`/`||`/ternary), Unknown beyond it. Sound
/// direction: only [`K3::False`] closes the route, only [`K3::True`]
/// proves it open — an Unknown gate is decided NEITHER way.
fn eval_consent(e: &Expr, prompt: &str, env: &RefusalEnv) -> K3 {
    match e {
        Expr::Lit(Literal::Bool(v)) => k3(*v),
        Expr::Not(inner) => eval_consent(inner, prompt, env).negate(),
        Expr::And(x, y) => eval_consent(x, prompt, env).and(eval_consent(y, prompt, env)),
        Expr::Or(x, y) => eval_consent(x, prompt, env).or(eval_consent(y, prompt, env)),
        Expr::Ternary { cond, then, else_ } => match eval_consent(cond, prompt, env) {
            K3::True => eval_consent(then, prompt, env),
            K3::False => eval_consent(else_, prompt, env),
            K3::Unknown => K3::Unknown,
        },
        Expr::Relation { op, lhs, rhs } => eval_relation(*op, lhs, rhs, prompt, env),
        _ if is_output_ref(e, prompt, env) => K3::False,
        _ => K3::Unknown,
    }
}

/// A relation — exact when both sides resolve to literals (the answer
/// IS the literal `false` under this evaluation), Unknown beyond.
fn eval_relation(op: RelOp, lhs: &Expr, rhs: &Expr, prompt: &str, env: &RefusalEnv) -> K3 {
    let l = resolve_lit(lhs, prompt, env);
    match (op, l) {
        (RelOp::Eq, Some(l)) => match resolve_lit(rhs, prompt, env) {
            Some(r) => k3(l == r),
            None => K3::Unknown,
        },
        (RelOp::Ne, Some(l)) => match resolve_lit(rhs, prompt, env) {
            Some(r) => k3(l != r),
            None => K3::Unknown,
        },
        (RelOp::In, Some(l)) => match rhs {
            Expr::List(items) => {
                let lits: Option<Vec<&Literal>> = items
                    .iter()
                    .map(|i| match i {
                        Expr::Lit(lit) => Some(lit),
                        _ => None,
                    })
                    .collect();
                match lits {
                    Some(lits) => k3(lits.iter().any(|lit| **lit == l)),
                    None => K3::Unknown,
                }
            }
            _ => K3::Unknown,
        },
        _ => K3::Unknown,
    }
}

/// A sub-expression resolved to a literal — the prompt's answer resolves
/// to `false` and its status to `"success"` BY CONSTRUCTION of this
/// evaluation.
fn resolve_lit(e: &Expr, prompt: &str, env: &RefusalEnv) -> Option<Literal> {
    match e {
        Expr::Lit(l) => Some(l.clone()),
        _ if is_output_ref(e, prompt, env) => Some(Literal::Bool(false)),
        _ if is_status_ref(e, prompt, env) => Some(Literal::Str("success".to_owned())),
        _ => None,
    }
}

fn k3(v: bool) -> K3 {
    if v { K3::True } else { K3::False }
}

/// The blocking row (NEP-0020) — names the sink AND the gate, teaches
/// the affirmative pattern (the human-gated-ship template's shape).
fn consent_finding(prompt: &str, sink: &str) -> ConsentFinding {
    ConsentFinding {
        prompt: prompt.to_owned(),
        sink: sink.to_owned(),
        detail: format!(
            "task `{sink}` runs on a route from confirm gate `{prompt}` that provably never \
             gates on the answer — a REFUSED confirm settles success with value false (the \
             Deny lives in the approval attestation only), so the effect fires on 'no' \
             (NEP-0020 · false triggers exactly zero effects) — fix: bind the answer and \
             gate on it: `with: {{ go: \"${{{{ tasks.{prompt}.output }}}}\" }}` + \
             `when: ${{{{ with.go == true }}}}` (the human-gated-ship pattern)"
        ),
    }
}

/// The advisory row for the UNPROVEN case — the gate may well consume
/// the answer through a shape the fragment cannot evaluate, so the lane
/// teaches rather than refuses (sound, never a false red).
fn consent_hint(prompt: &str, sink: &str) -> Hint {
    Hint {
        kind: "consent",
        task: sink.to_owned(),
        advice: format!(
            "`{sink}` sits on a route from `{prompt}` the checker cannot PROVE consumes the \
             answer (a nested binding · a non-fragment expression) — if it does not, a \
             REFUSED confirm still settles success with value false and the effect fires on \
             'no'; make the consumption provable: \
             `with: {{ go: \"${{{{ tasks.{prompt}.output }}}}\" }}` + \
             `when: ${{{{ with.go == true }}}}` (the human-gated-ship pattern)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::check;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// The P0-2 fixture: a confirm whose `default:` answers false
    /// UNATTENDED — and a bare state edge carries that refusal straight
    /// into the irreversible exec.
    const BARE: &str = "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n";

    /// NEP-0020 · the closure: the PROVEN non-affirmative route is a
    /// NIKA-SEC-014 refusal (the hint of 2026-07-30 escalates) — the
    /// check fails, the code rides every surface (the findings fold ·
    /// the extra-conformance list), and the witness names gate + sink.
    #[test]
    fn the_proven_non_affirmative_route_refuses_with_sec_014() {
        let r = report(BARE);
        assert!(
            !r.is_clean(),
            "a rubber-stamp route fails the check (NEP-0020): {r:?}"
        );
        let codes: Vec<String> = r
            .extra_conformance_codes()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            codes.iter().any(|c| c == "NIKA-SEC-014"),
            "the refusal carries its code: {codes:?}"
        );
        let row = r
            .findings
            .iter()
            .find(|f| f.kind == "consent" && f.code.as_deref() == Some("NIKA-SEC-014"))
            .unwrap_or_else(|| panic!("the blocking row in findings[]: {:#?}", r.findings));
        assert_eq!(row.task.as_deref(), Some("push"), "the sink is the witness");
        assert!(
            row.message.contains("ask") && row.message.contains("push"),
            "gate + sink named: {}",
            row.message
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "the proven case is a refusal, not a plea: {:?}",
            r.hints
        );
    }

    /// The bare state edge into an exec — the refusal settles success,
    /// the edge admits it, the push fires. The finding names the sink
    /// and teaches the affirmative pattern.
    #[test]
    fn the_finding_names_gate_sink_and_the_repair() {
        let r = report(BARE);
        assert_eq!(r.consent_findings.len(), 1, "{:?}", r.consent_findings);
        let f = &r.consent_findings[0];
        assert_eq!(f.prompt, "ask");
        assert_eq!(f.sink, "push");
        assert!(
            f.detail.contains("tasks.ask.output") && f.detail.contains("with.go == true"),
            "the affirmative pattern is the repair: {}",
            f.detail
        );
        assert_eq!(crate::ConsentFinding::WIRE_CODE, "NIKA-SEC-014");
    }

    /// An UNDECIDABLE gate stays advisory (sound — no false red): the
    /// answer reaches the `when:` through a NESTED template the consent
    /// fragment cannot evaluate, so the defect is unproven and the lane
    /// speaks the hint, never the code.
    #[test]
    fn an_undecidable_gate_stays_advisory() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { go: \"answer=${{ tasks.ask.output }}\" }\n    when: ${{ with.go == 'answer=true' }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(r.is_clean(), "unproven is advisory, never a refusal: {r:?}");
        assert!(
            r.hints.iter().any(|h| h.kind == "consent"),
            "the uncertain case keeps the hint: {:?}",
            r.hints
        );
    }

    /// The refusal settles SUCCESS — an edge that admits only `failure`
    /// cannot carry it. The walk reads the pass-set: no route, no
    /// finding, no hint (predicate-blind would be a false red here).
    #[test]
    fn a_failure_only_edge_carries_no_refusal() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    after: { ask: failure }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.is_clean(),
            "the failure edge admits nothing of a refusal: {r:?}"
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "not even a hint — there is no route: {:?}",
            r.hints
        );
    }

    /// The same non-affirmative route feeds the risk grade: a human gate
    /// that cannot block is a High signal (P0-2 · risk.rs — the finding
    /// and the hint both lift it).
    #[test]
    fn the_non_affirmative_gate_is_a_high_risk_signal() {
        let r = report(BARE);
        assert!(
            crate::risk_grade(&r) >= crate::RiskGrade::High,
            "a rubber-stamp route lifts the grade: {:?}",
            crate::risk_grade(&r)
        );
    }

    /// The house pattern — `with: go` + `when: ${{ with.go == true }}` —
    /// consumes the answer and proves false on the refusal: silence, and
    /// the grade stays on its own rung.
    #[test]
    fn affirmative_consumption_silences_the_lane() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(r.is_clean(), "the answer is consumed affirmatively: {r:?}");
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "no advisory either: {:?}",
            r.hints
        );
        assert_eq!(
            crate::risk_grade(&r),
            crate::RiskGrade::Supervised,
            "no consent signal, no bump"
        );
    }

    /// `when: false` closes a route by construction — the never-pattern
    /// is the second lawful closure (NEP-0020). (The dead-task lane
    /// `NIKA-DAG-006` flags the `when: false` itself — the CONSENT read
    /// is what stays silent here.)
    #[test]
    fn a_never_run_task_closes_the_route() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  mid:\n    after: { ask: success }\n    when: false\n    infer: { prompt: \"x\", max_tokens: 9 }\n  push:\n    after: { mid: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings.is_empty(),
            "when: false closes the route by construction: {:?}",
            r.consent_findings
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "no advisory either: {:?}",
            r.hints
        );
    }

    /// The bypass case: ONE affirmative route does not discharge the
    /// OTHER — the sink reached over the bare second route refuses.
    #[test]
    fn a_bypass_route_refuses_its_own_sink() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\", \"curl\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"ship?\", default: false }\n  act:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"git\", \"push\"] }\n  ship:\n    after: { ask: success }\n    exec: { command: [\"curl\", \"-X\", \"POST\", \"https://example.com/hook\"] }\n",
        );
        assert_eq!(
            r.consent_findings.len(),
            1,
            "only the bypassed sink refuses: {:?}",
            r.consent_findings
        );
        assert_eq!(r.consent_findings[0].sink, "ship");
    }

    /// A `when:` that reads the prompt's STATUS instead of its answer is
    /// PROVEN open, not affirmative — the refusal settles `"success"`,
    /// so the gate holds under every refusal and the route refuses
    /// (spec fixture `013-consent-status-gate-is-not-consent`).
    #[test]
    fn a_status_gate_is_proven_open_not_consent() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { st: \"${{ tasks.ask.status }}\" }\n    when: ${{ with.st == 'success' }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings
                .iter()
                .any(|f| f.sink == "push" && f.prompt == "ask"),
            "the status is decidable — and it is not consent: {:?}",
            r.consent_findings
        );
    }

    /// The status substitution is honest BOTH ways: a gate on
    /// `with.st == 'failure'` proves FALSE under the refusal — the route
    /// is closed and the lane is silent (a true negative the
    /// output-only evaluator could not see).
    #[test]
    fn a_status_gate_proven_false_closes_the_route() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { st: \"${{ tasks.ask.status }}\" }\n    when: ${{ with.st == 'failure' }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings.is_empty() && !r.hints.iter().any(|h| h.kind == "consent"),
            "proven false is closed: {:?} · {:?}",
            r.consent_findings,
            r.hints
        );
    }

    /// A `when:` that references the answer but cannot be FALSE on a
    /// refusal is proven open (`go == true || go == false` holds under
    /// every answer) — the tautology refuses.
    #[test]
    fn a_when_true_on_false_is_proven_open() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true || with.go == false }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings.iter().any(|f| f.sink == "push"),
            "a tautology over the answer blocks nothing: {:?}",
            r.consent_findings
        );
    }

    /// The ungated route is transitive: an intermediate pure-compute
    /// task does not launder the consent — the egress sink downstream
    /// refuses.
    #[test]
    fn a_transitive_ungated_route_refuses_the_egress_sink() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  mid:\n    after: { ask: success }\n    infer: { prompt: \"summarize\", max_tokens: 9 }\n  push:\n    after: { mid: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(
            r.consent_findings.len(),
            1,
            "the infer is not egress — only the push refuses: {:?}",
            r.consent_findings
        );
        assert_eq!(r.consent_findings[0].sink, "push");
    }

    /// A BLOCKING prompt (no `default:`) needs the same consumption: the
    /// interactive « no » settles success-with-false exactly like the
    /// unattended default.
    #[test]
    fn a_blocking_confirm_also_refuses_the_bare_route() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\" }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings.iter().any(|f| f.sink == "push"),
            "blocking is not affirmative: {:?}",
            r.consent_findings
        );
    }

    /// The nearest gate owns its closure (the approval-batch precedent):
    /// a second confirm on the route cuts the FIRST prompt's walk — the
    /// bare route past the second gate is the second gate's own refusal.
    #[test]
    fn the_nearest_gate_owns_its_closure() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  first:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"one?\", default: false }\n  second:\n    after: { first: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"two?\", default: false }\n  push:\n    after: { second: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(
            r.consent_findings.len(),
            1,
            "one refusal — the nearest gate's own: {:?}",
            r.consent_findings
        );
        assert_eq!(
            r.consent_findings[0].prompt, "second",
            "the second gate owns the closure"
        );
    }

    /// `mode: choice` is OUT OF SCOPE — its answer is a string and the
    /// affirmative pattern differs (`with.answer == 'yes'`); the lane
    /// claims nothing there (silence, never wrong).
    #[test]
    fn a_choice_prompt_is_out_of_scope() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: choice, message: \"push?\", choices: [\"no\", \"yes\"], default: \"no\" }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.is_clean(),
            "choice answers are not the confirm contract: {r:?}"
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "{:?}",
            r.hints
        );
    }
}
