// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `when:`-gate substrate — Kleene-3 logic, the gate island, and the
//! REFUSAL SUBSTITUTION: what a task's `when:` evaluates to once ONE
//! confirm gate has answered « no » (it settled `success`, its value is
//! `false` · spec `10-authority.md` §the affirmative-consent law).
//!
//! Descended from `nika-check` (`reach.rs` · `consent.rs`) at the 15k
//! prod-LOC wall — pure AST reads, moved as they stood: the dead-gate lane
//! and the consent lane both judge `when:` gates, and the judgment ladder
//! keeps the VERDICTS (which task is a sink · what refuses · what advises).
//!
//! Two readings of one gate, and they are NOT interchangeable ·
//!
//! - [`gate_verdict`] — Kleene: `Closed` means the task can NEVER settle
//!   `success` on a refusal. It does not mean the task SKIPS: the runtime
//!   evaluates the left operand of `&&`/`||` first, so `<error> && false`
//!   fails the task (`NIKA-VAR-006`) where Kleene reads plain false.
//! - [`gate_certain`] — total: `Closed` means the task certainly settles
//!   `skipped` once admitted, `Open` that it certainly reaches its verb.
//!   Every step the runtime takes BEFORE the verb (bindings · `when:` ·
//!   the `for_each` collection) is proven unable to error, in the order
//!   the runtime takes it. This is the only reading a WITNESS may use.

use std::collections::{BTreeMap, BTreeSet};

use nika_schema::expression::{Expr, Literal, NamespaceRef, RelOp, expr_refs, scan_templates};
use nika_schema::raw::RawTask;
use nika_schema::types::WhenGate;

/// Kleene three-valued logic. `#[non_exhaustive]` per FCI-002: a consumer
/// in another crate reads any value it does not name as [`K3::Unknown`] —
/// the one fallback that is never evidence for either side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum K3 {
    /// Proven true.
    True,
    /// Proven false.
    False,
    /// Not decided — never evidence for either side.
    Unknown,
}

impl K3 {
    /// `!x` — an undecided operand stays undecided.
    #[must_use]
    pub fn negate(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
    /// `x && y` as a VALUE: false wins over undecided. It says nothing
    /// about evaluation ORDER — see [`gate_certain`] for that.
    #[must_use]
    pub fn and(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }
    /// `x || y` as a VALUE: true wins over undecided.
    #[must_use]
    pub fn or(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }
}

/// Parse the single boolean island of a `when:` gate.
#[must_use]
pub fn parse_gate(src: &str) -> Option<Expr> {
    let islands = scan_templates(src).ok()?;
    let island = islands.into_iter().next()?;
    Some(island.expr)
}

/// The task's `when:` under THIS prompt's refusal — the three fates
/// (spec 10 §the affirmative-consent law). What each one PROVES depends on
/// the reading that produced it ([`gate_verdict`] · [`gate_certain`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Gate {
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

/// The task's `when:` under the refusal — see [`Gate`]. The gate
/// consumes the answer through the EXACT carriers the substitution
/// resolves; a carrier the fragment cannot resolve (a nested template ·
/// another field) makes any gate reading it unproven.
///
/// The Kleene reading: `Closed` = the task can never settle `success` on
/// a refusal (it skips, or its gate errors and it FAILS) · `Open` = the
/// gate's VALUE is true. Enough to close a route; never enough to witness
/// what runs — that is [`gate_certain`].
#[must_use]
pub fn gate_verdict(task: &RawTask, prompt: &str) -> Gate {
    gate_under(task, prompt, false)
}

/// The same gate read for a WITNESS: `Closed` = once admitted the task
/// certainly settles `skipped` · `Open` = it certainly reaches its verb ·
/// `Unclear` = anything the runtime does before the verb may error, so
/// nothing is proven (the dispatch order of spec 03: GATE → bindings →
/// `when:` → verb, and an error at either middle step settles `failure`).
///
/// Total, not Kleene: the runtime evaluates the LEFT operand of `&&`/`||`
/// first, so an undecided left is a possible `NIKA-VAR-006` whatever the
/// right says (`<cross-type> && false` FAILS the task, it does not skip
/// it), and `==` across two classes errors instead of reading false. A
/// `for_each` collection and a binding that navigates or computes are
/// pre-verb steps the fragment does not evaluate — undecided, never
/// assumed.
#[must_use]
pub fn gate_certain(task: &RawTask, prompt: &str) -> Gate {
    if task.for_each.is_some() || !task.with.iter().all(|(_, v)| total_binding(&v.value)) {
        return Gate::Unclear;
    }
    gate_under(task, prompt, true)
}

/// A `with:` value whose evaluation cannot error: no island at all, or
/// islands that are PLAIN reads (nested values included).
fn total_binding(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => {
            scan_templates(s).is_ok_and(|islands| islands.iter().all(|i| plain_read(&i.expr)))
        }
        serde_json::Value::Array(items) => items.iter().all(total_binding),
        serde_json::Value::Object(map) => map.values().all(total_binding),
        _ => true,
    }
}

/// A read that is defined whatever its producer settled: a literal ·
/// `inputs.<x>` / `const.<x>` / `group.<g>` · `tasks.<id>.<field>` (a
/// skipped producer reads defined-null · spec 03). One step deeper
/// (`tasks.a.output.path`) navigates a value that may be null.
fn plain_read(e: &Expr) -> bool {
    let Some(base) = named_step(e) else {
        return matches!(e, Expr::Lit(_));
    };
    match base {
        Expr::Ident(root) => matches!(root.as_str(), "inputs" | "const" | "group"),
        record => matches!(named_step(record), Some(Expr::Ident(root)) if root == "tasks"),
    }
}

/// `<base>.<name>` or `<base>['<name>']` → the base.
fn named_step(e: &Expr) -> Option<&Expr> {
    match e {
        Expr::Member { base, .. } => Some(base),
        Expr::Index { base, index } if matches!(index.as_ref(), Expr::Lit(Literal::Str(_))) => {
            Some(base)
        }
        _ => None,
    }
}

/// The ONE gate evaluation both readings share — `total` selects the
/// reading (see [`gate_certain`]).
fn gate_under(task: &RawTask, prompt: &str, total: bool) -> Gate {
    let Some(when) = task.when.as_ref() else {
        return Gate::Open;
    };
    let src = match &when.value {
        WhenGate::Literal(v) => return if *v { Gate::Open } else { Gate::Closed },
        WhenGate::Expr(src) => src,
    };
    let Some(expr) = parse_gate(src) else {
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
    match eval_consent(&expr, prompt, &env, total) {
        K3::False => Gate::Closed,
        K3::True => Gate::Open,
        K3::Unknown => Gate::Unclear,
    }
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
///
/// `total` reads the same fragment in the runtime's evaluation ORDER
/// (`nika-cel` · the left operand of `&&`/`||` is always evaluated, the
/// right only when the left does not decide): an Unknown that WOULD be
/// evaluated is a possible error, so it decides nothing — where Kleene
/// lets a false right operand win.
fn eval_consent(e: &Expr, prompt: &str, env: &RefusalEnv, total: bool) -> K3 {
    match e {
        Expr::Lit(Literal::Bool(v)) => k3(*v),
        Expr::Not(inner) => eval_consent(inner, prompt, env, total).negate(),
        Expr::And(x, y) => match eval_consent(x, prompt, env, total) {
            K3::Unknown if total => K3::Unknown,
            left => left.and(eval_consent(y, prompt, env, total)),
        },
        Expr::Or(x, y) => match eval_consent(x, prompt, env, total) {
            K3::Unknown if total => K3::Unknown,
            left => left.or(eval_consent(y, prompt, env, total)),
        },
        Expr::Ternary { cond, then, else_ } => match eval_consent(cond, prompt, env, total) {
            K3::True => eval_consent(then, prompt, env, total),
            K3::False => eval_consent(else_, prompt, env, total),
            K3::Unknown => K3::Unknown,
        },
        Expr::Relation { op, lhs, rhs } => eval_relation(*op, lhs, rhs, prompt, env, total),
        _ if is_output_ref(e, prompt, env) => K3::False,
        _ => K3::Unknown,
    }
}

/// A relation — exact when both sides resolve to literals (the answer
/// IS the literal `false` under this evaluation), Unknown beyond.
///
/// `total`: `==`/`!=` across two classes is `NIKA-VAR-006` at run, never
/// `false` (no implicit coercion · `null` excepted, the defined-null
/// test) — so it decides nothing. `in` over a literal list cannot error:
/// the runtime reads a mismatched element as « not the needle ».
fn eval_relation(
    op: RelOp,
    lhs: &Expr,
    rhs: &Expr,
    prompt: &str,
    env: &RefusalEnv,
    total: bool,
) -> K3 {
    let l = resolve_lit(lhs, prompt, env);
    match (op, l) {
        (RelOp::Eq | RelOp::Ne, Some(l)) => match resolve_lit(rhs, prompt, env) {
            Some(r) if total && !same_class(&l, &r) => K3::Unknown,
            Some(r) => k3((l == r) == (op == RelOp::Eq)),
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

/// Two literals `==` can compare without erroring: one class, or a `null`
/// side. An int against a float is left undecided — the runtime compares
/// them on the number line, the literal `==` here by representation.
fn same_class(l: &Literal, r: &Literal) -> bool {
    matches!(l, Literal::Null)
        || matches!(r, Literal::Null)
        || std::mem::discriminant(l) == std::mem::discriminant(r)
}

fn k3(v: bool) -> K3 {
    if v { K3::True } else { K3::False }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    /// Both readings of task `t` under the refusal of gate `ask` — the
    /// task body is the fixture (`with:` · `when:` · `for_each:`).
    fn readings(body: &str) -> (Gate, Gate) {
        let yaml = format!("nika: t\ntasks:\n  t:\n{body}    exec: {{ command: [\"true\"] }}\n");
        let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let task = &wf.tasks[0].value;
        (gate_verdict(task, "ask"), gate_certain(task, "ask"))
    }

    const GO: &str = "go: \"${{ tasks.ask.output }}\"";

    #[test]
    fn the_house_pattern_is_closed_in_both_readings() {
        let body = format!("    with: {{ {GO} }}\n    when: ${{{{ with.go == true }}}}\n");
        assert_eq!(readings(&body), (Gate::Closed, Gate::Closed));
        assert_eq!(readings("    when: false\n"), (Gate::Closed, Gate::Closed));
        assert_eq!(readings(""), (Gate::Open, Gate::Open));
    }

    /// The runtime evaluates the LEFT of `&&` first. An undecided left may
    /// be an error — Kleene still reads the VALUE false (never `success`),
    /// the total reading refuses to call it a skip. The same operands the
    /// other way round short-circuit on the false answer: a sure skip.
    #[test]
    fn and_is_read_in_the_runtime_order() {
        let with = format!("    with: {{ {GO}, report: \"text\" }}\n");
        let undecided_first =
            format!("{with}    when: ${{{{ with.report > 0 && with.go == true }}}}\n");
        assert_eq!(readings(&undecided_first), (Gate::Closed, Gate::Unclear));
        let answer_first =
            format!("{with}    when: ${{{{ with.go == true && with.report > 0 }}}}\n");
        assert_eq!(readings(&answer_first), (Gate::Closed, Gate::Closed));
    }

    #[test]
    fn or_is_read_in_the_runtime_order() {
        let with = format!("    with: {{ {GO}, report: \"text\" }}\n");
        let undecided_first =
            format!("{with}    when: ${{{{ with.report > 0 || with.go == false }}}}\n");
        assert_eq!(readings(&undecided_first), (Gate::Open, Gate::Unclear));
        let answer_first =
            format!("{with}    when: ${{{{ with.go == false || with.report > 0 }}}}\n");
        assert_eq!(readings(&answer_first), (Gate::Open, Gate::Open));
    }

    /// `==` across two classes is `NIKA-VAR-006` at run, never `false` —
    /// `null` excepted (the defined-null test), and `in` over a literal
    /// list cannot error at all.
    #[test]
    fn a_cross_type_compare_decides_nothing_for_a_witness() {
        let with = format!("    with: {{ {GO} }}\n");
        let cross = format!("{with}    when: ${{{{ with.go == 'yes' }}}}\n");
        assert_eq!(readings(&cross), (Gate::Closed, Gate::Unclear));
        let null_test = format!("{with}    when: ${{{{ with.go != null }}}}\n");
        assert_eq!(readings(&null_test), (Gate::Open, Gate::Open));
        let membership = format!("{with}    when: ${{{{ with.go in [true, 'yes'] }}}}\n");
        assert_eq!(readings(&membership), (Gate::Closed, Gate::Closed));
    }

    /// Bindings and the `for_each` collection are evaluated BEFORE the
    /// verb: a plain read is defined whatever its producer settled, a
    /// navigation or a computation may error — at any nesting depth.
    #[test]
    fn a_pre_verb_step_that_may_error_is_never_certain() {
        let plain = "    with: { v: \"${{ tasks.a.output }}\", k: \"n=${{ inputs.k }}\", c: 3 }\n";
        assert_eq!(readings(plain), (Gate::Open, Gate::Open));
        let navigates = "    with: { p: \"${{ tasks.a.output.path }}\" }\n";
        assert_eq!(readings(navigates), (Gate::Open, Gate::Unclear));
        let computes = "    with: { n: \"${{ size(tasks.a.output) }}\" }\n";
        assert_eq!(readings(computes), (Gate::Open, Gate::Unclear));
        let nested = "    with: { o: { deep: [\"${{ tasks.a.output.x }}\"] } }\n";
        assert_eq!(readings(nested), (Gate::Open, Gate::Unclear));
        let fans_out = "    with: { v: \"${{ tasks.a.output }}\" }\n    for_each: { items: \"${{ with.v }}\" }\n";
        assert_eq!(readings(fans_out), (Gate::Open, Gate::Unclear));
    }
}
