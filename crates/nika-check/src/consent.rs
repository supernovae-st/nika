// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The affirmative-consent lane (P0-2 of the 2026-07-30 UX audit) —
//! **advisory**: a `consent` HINT per (prompt, sink) pair, never a
//! finding. The blocking closure (a dedicated `NIKA-*` refusal) is
//! DEFERRED to the spec pass — a hint needs no spec code (the
//! `retry-effects` precedent), and this module changes no verdict.
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
//! The human gate is only credited when every route to the effect
//! consumes the prompt's output and PROVES false on the refusal — the
//! house pattern (`nika-pack` `human-gated-ship.nika.yaml`):
//! `with: { go: "${{ tasks.ask.output }}" }` + `when: ${{ with.go == true }}`.
//!
//! Method: per confirm-mode `invoke: nika:prompt`, a BFS over the
//! analyzer's derived edges; an affirmative gate (its `when:` evaluates
//! to `K3::False` with the answer substituted by `false`) or a closer
//! confirm prompt CUTS the route (the nearest gate owns its closure —
//! the approval-batch precedent). Any egress-capable task (the ONE
//! effect table, `trifecta::egress_capable`) reached on an uncut route
//! earns the hint. The lane is gated on a valid DAG (the caller's
//! IFC/policy gating) and skips past the analysis task cap (the O(P·E)
//! per-prompt walk shares the `DoS` floor).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nika_schema::expression::{Expr, Literal, RelOp, scan_templates};
use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use nika_schema::types::WhenGate;

use crate::analyzer::Edge;
use crate::hints::Hint;
use crate::reach::K3;

/// Judge the affirmative-consent lane over the derived graph. Empty
/// unless a confirm-mode prompt reaches an egress-capable descendant
/// over a route that never consumes the answer affirmatively.
pub(crate) fn scan_consent(wf: &RawWorkflow, edges: &[Edge]) -> Vec<Hint> {
    if wf.tasks.len() > crate::analysis::ANALYSIS_TASK_CAP {
        return Vec::new();
    }
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); wf.tasks.len()];
    for e in edges {
        children[e.from].push(e.to);
    }
    let mut hints = Vec::new();
    for (idx, task) in wf.tasks.iter().enumerate() {
        if !is_confirm_prompt(&task.value) {
            continue;
        }
        let prompt = task.value.id.value.as_str();
        // BFS over the derived edges — an affirmative gate or a closer
        // confirm prompt cuts the route; everything else forwards it.
        let mut seen: BTreeSet<usize> = BTreeSet::from([idx]);
        let mut queue: VecDeque<usize> = children[idx].iter().copied().collect();
        while let Some(n) = queue.pop_front() {
            if !seen.insert(n) {
                continue;
            }
            let t = &wf.tasks[n].value;
            if is_confirm_prompt(t) || never_runs(t) || affirmative(t, prompt) {
                continue;
            }
            if crate::trifecta::egress_capable(&t.action) {
                hints.push(consent_hint(prompt, &t.id.value));
            }
            queue.extend(children[n].iter().copied());
        }
    }
    hints
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

/// `when: false` — the documented never-pattern: the task cannot run,
/// so no route THROUGH it carries the refusal anywhere.
fn never_runs(task: &RawTask) -> bool {
    task.when
        .as_ref()
        .is_some_and(|w| matches!(w.value, WhenGate::Literal(false)))
}

/// The task's `when:` PROVES false when this prompt's answer is false:
/// the gate consumes the output (a `with:` binding carrying exactly
/// `${{ tasks.<prompt>.output }}`, or the direct reference) and the
/// whole expression evaluates to [`K3::False`] under output = false.
/// Shared with the policy lane (`check/policy.rs`) — the ONE evaluator
/// both the advisory hint and the hard `require.human_gate_before`
/// judge read (the juge pur in nika-cap cannot parse expressions; the
/// projection carries its verdict).
pub(crate) fn affirmative(task: &RawTask, prompt: &str) -> bool {
    let Some(when) = task.when.as_ref() else {
        return false;
    };
    let WhenGate::Expr(src) = &when.value else {
        return false;
    };
    let Some(expr) = crate::reach::parse_gate(src) else {
        return false;
    };
    eval_consent(&expr, prompt, &output_bindings(task)) == K3::False
}

/// The with-bindings carrying EXACTLY one bare `${{ tasks.<id>.output }}`
/// island — the W2 observation idiom, output flavor (the status-flavor
/// mirror lives in `reach.rs::status_bindings`).
fn output_bindings(task: &RawTask) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
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
        if let Some(id) = output_target(&island.expr) {
            out.insert(key.value.clone(), id);
        }
    }
    out
}

/// The expr IS exactly `tasks.<id>.output` (member or index form).
fn output_target(e: &Expr) -> Option<String> {
    let Expr::Member { base, field } = e else {
        return None;
    };
    if field != "output" {
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

/// The gate sub-expression reads THIS prompt's answer — the direct
/// `tasks.<prompt>.output` form or a `with:` binding carrying it.
fn is_output_ref(e: &Expr, prompt: &str, b: &BTreeMap<String, String>) -> bool {
    if output_target(e).as_deref() == Some(prompt) {
        return true;
    }
    match e {
        Expr::Member { base, field } => match base.as_ref() {
            Expr::Ident(root) if root == "with" => b.get(field).is_some_and(|id| id == prompt),
            _ => false,
        },
        Expr::Index { base, index } => match (base.as_ref(), index.as_ref()) {
            (Expr::Ident(root), Expr::Lit(Literal::Str(name))) if root == "with" => {
                b.get(name).is_some_and(|id| id == prompt)
            }
            _ => false,
        },
        _ => false,
    }
}

/// Kleene-3 evaluation of a `when:` gate with THIS prompt's answer
/// substituted by `false` — exact over the consent fragment (boolean
/// literals · `==`/`!=`/`in` on resolved literals · `!`/`&&`/`||`/
/// ternary), Unknown beyond it. Sound direction: only [`K3::False`]
/// credits the gate (an Unknown gate is never called affirmative).
fn eval_consent(e: &Expr, prompt: &str, b: &BTreeMap<String, String>) -> K3 {
    match e {
        Expr::Lit(Literal::Bool(v)) => k3(*v),
        Expr::Not(inner) => eval_consent(inner, prompt, b).negate(),
        Expr::And(x, y) => eval_consent(x, prompt, b).and(eval_consent(y, prompt, b)),
        Expr::Or(x, y) => eval_consent(x, prompt, b).or(eval_consent(y, prompt, b)),
        Expr::Ternary { cond, then, else_ } => match eval_consent(cond, prompt, b) {
            K3::True => eval_consent(then, prompt, b),
            K3::False => eval_consent(else_, prompt, b),
            K3::Unknown => K3::Unknown,
        },
        Expr::Relation { op, lhs, rhs } => eval_relation(*op, lhs, rhs, prompt, b),
        _ if is_output_ref(e, prompt, b) => K3::False,
        _ => K3::Unknown,
    }
}

/// A relation — exact when both sides resolve to literals (the answer
/// IS the literal `false` under this evaluation), Unknown beyond.
fn eval_relation(
    op: RelOp,
    lhs: &Expr,
    rhs: &Expr,
    prompt: &str,
    b: &BTreeMap<String, String>,
) -> K3 {
    let l = resolve_lit(lhs, prompt, b);
    match (op, l) {
        (RelOp::Eq, Some(l)) => match resolve_lit(rhs, prompt, b) {
            Some(r) => k3(l == r),
            None => K3::Unknown,
        },
        (RelOp::Ne, Some(l)) => match resolve_lit(rhs, prompt, b) {
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

/// A sub-expression resolved to a literal — the prompt's answer
/// resolves to `false` BY CONSTRUCTION of this evaluation.
fn resolve_lit(e: &Expr, prompt: &str, b: &BTreeMap<String, String>) -> Option<Literal> {
    match e {
        Expr::Lit(l) => Some(l.clone()),
        _ if is_output_ref(e, prompt, b) => Some(Literal::Bool(false)),
        _ => None,
    }
}

fn k3(v: bool) -> K3 {
    if v { K3::True } else { K3::False }
}

/// The advisory row — names the sink AND the prompt, teaches the
/// affirmative pattern (the human-gated-ship template's shape).
fn consent_hint(prompt: &str, sink: &str) -> Hint {
    Hint {
        kind: "consent",
        task: sink.to_owned(),
        advice: format!(
            "`{sink}` runs on a route from `{prompt}` that never consumes the answer — a \
             REFUSED confirm still settles success with value false (the Deny lives in the \
             approval attestation only), so a bare `after: {{ {prompt}: success }}` lets the \
             effect through; bind the answer and gate on it: \
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
    const BARE: &str = "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n";

    /// A bare `after: { ask: success }` into an exec — the refusal
    /// settles success, the edge admits it, the push fires. The hint
    /// names the sink and teaches the affirmative pattern.
    #[test]
    fn bare_state_edge_after_a_refusable_confirm_is_a_consent_hint() {
        let r = report(BARE);
        let consent: Vec<_> = r.hints.iter().filter(|h| h.kind == "consent").collect();
        assert_eq!(consent.len(), 1, "one hint, naming the sink: {:?}", r.hints);
        assert_eq!(consent[0].task, "push");
        assert!(
            consent[0].advice.contains("ask") && consent[0].advice.contains("push"),
            "prompt and sink named: {}",
            consent[0].advice
        );
    }

    /// The same non-affirmative gate feeds the risk grade: a human gate
    /// that cannot block is a High signal (P0-2 · risk.rs).
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
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "the answer is consumed affirmatively: {:?}",
            r.hints
        );
        assert_eq!(
            crate::risk_grade(&r),
            crate::RiskGrade::Supervised,
            "no consent signal, no bump"
        );
    }

    /// The bypass case: ONE affirmative route does not discharge the
    /// OTHER — the sink reached over the bare second route is named.
    #[test]
    fn a_bypass_route_names_its_own_sink() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"git\", \"curl\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"ship?\", default: false }\n  act:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"git\", \"push\"] }\n  ship:\n    after: { ask: success }\n    exec: { command: [\"curl\", \"-X\", \"POST\", \"https://example.com/hook\"] }\n",
        );
        let consent: Vec<_> = r.hints.iter().filter(|h| h.kind == "consent").collect();
        assert_eq!(
            consent.len(),
            1,
            "only the bypassed sink is named: {:?}",
            r.hints
        );
        assert_eq!(consent[0].task, "ship");
        assert!(consent[0].advice.contains("ship"), "{}", consent[0].advice);
    }

    /// A `when:` that reads the prompt's STATUS instead of its answer is
    /// not affirmative — the gate consumes nothing of the consent.
    #[test]
    fn a_when_that_ignores_the_answer_is_not_affirmative() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { st: \"${{ tasks.ask.status }}\" }\n    when: ${{ with.st == 'success' }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.hints
                .iter()
                .any(|h| h.kind == "consent" && h.task == "push"),
            "the status is not the answer: {:?}",
            r.hints
        );
    }

    /// A `when:` that references the answer but cannot be FALSE on a
    /// refusal is not affirmative either (`go == true || go == false`
    /// holds under every answer).
    #[test]
    fn a_when_true_on_false_is_not_affirmative() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true || with.go == false }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.hints
                .iter()
                .any(|h| h.kind == "consent" && h.task == "push"),
            "a tautology over the answer blocks nothing: {:?}",
            r.hints
        );
    }

    /// The ungated route is transitive: an intermediate pure-compute
    /// task does not launder the consent — the egress sink downstream is
    /// named.
    #[test]
    fn a_transitive_ungated_route_names_the_egress_sink() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  mid:\n    after: { ask: success }\n    infer: { prompt: \"summarize\", max_tokens: 9 }\n  push:\n    after: { mid: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        let consent: Vec<_> = r.hints.iter().filter(|h| h.kind == "consent").collect();
        assert_eq!(
            consent.len(),
            1,
            "the infer is not egress — only the push is named: {:?}",
            r.hints
        );
        assert_eq!(consent[0].task, "push");
    }

    /// A BLOCKING prompt (no `default:`) needs the same consumption: the
    /// interactive « no » settles success-with-false exactly like the
    /// unattended default.
    #[test]
    fn a_blocking_confirm_also_needs_affirmative_consumption() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\" }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.hints
                .iter()
                .any(|h| h.kind == "consent" && h.task == "push"),
            "blocking is not affirmative: {:?}",
            r.hints
        );
    }

    /// The nearest gate owns its closure (the approval-batch precedent):
    /// a second confirm on the route cuts the FIRST prompt's walk — the
    /// bare route past the second gate is the second gate's own defect.
    #[test]
    fn the_nearest_gate_owns_its_closure() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  first:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"one?\", default: false }\n  second:\n    after: { first: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"two?\", default: false }\n  push:\n    after: { second: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        let consent: Vec<_> = r.hints.iter().filter(|h| h.kind == "consent").collect();
        assert_eq!(
            consent.len(),
            1,
            "one hint — the nearest gate's own: {:?}",
            r.hints
        );
        assert!(
            consent[0].advice.contains("second"),
            "the second gate owns the closure: {}",
            consent[0].advice
        );
    }

    /// `mode: choice` is OUT OF SCOPE — its answer is a string and the
    /// affirmative pattern differs (`with.answer == 'yes'`); the lane
    /// claims nothing there (silence, never wrong).
    #[test]
    fn a_choice_prompt_is_out_of_scope() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: choice, message: \"push?\", choices: [\"no\", \"yes\"], default: \"no\" }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "choice answers are not the confirm contract: {:?}",
            r.hints
        );
    }
}
