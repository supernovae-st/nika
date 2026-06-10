// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `one-obvious-way` rule set — spec `03-dag.md` §One obvious way.
//!
//! Seven control-flow preference rules · the spec table is « normative
//! for linters » · emitted as warnings in table order
//! (`one-obvious-way/001` … `/007`) · never hard errors.
//!
//! Each rule fires only on a PRECISE static signature (low
//! false-positive contract — pinned by `tests/lints_one_obvious_way.rs`).
//! Where the spec row describes intent the validator cannot decide
//! statically (« a mere value » · « manual sharding »), the rule
//! implements a documented conservative subset.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::expression::{Expr, Literal, NamespaceRef, RelOp, expr_refs, scan_templates};
use crate::raw::{RawAction, RawTask, RawWorkflow};
use crate::source::Span;
use crate::types::OnError;

/// One advisory finding from a lint pass.
///
/// Warnings only — a workflow with lints is still valid (spec
/// `03-dag.md` · « the discouraged forms are legal · just not
/// canonical »).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Lint {
    /// The stable rule id (`one-obvious-way/001` … `/007`).
    pub rule: &'static str,
    /// The task the finding is attached to.
    pub task_id: String,
    /// The flagged task's id span (diagnostic anchor).
    pub span: Span,
    /// What was detected (the discouraged form).
    pub message: String,
    /// The canonical form to use instead.
    pub suggestion: String,
}

impl Lint {
    /// Create a lint record.
    #[must_use]
    pub fn new(
        rule: &'static str,
        task_id: String,
        span: Span,
        message: String,
        suggestion: String,
    ) -> Self {
        Self {
            rule,
            task_id,
            span,
            message,
            suggestion,
        }
    }
}

/// Run the 7 `one-obvious-way` preference rules over a parsed workflow.
///
/// Output is deterministic · sorted by (task position · rule id).
#[must_use]
pub fn one_obvious_way(wf: &RawWorkflow) -> Vec<Lint> {
    let tasks: Vec<&RawTask> = wf.tasks.iter().map(|t| &t.value).collect();
    let index: BTreeMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.value.as_str(), i))
        .collect();

    let mut lints = Vec::new();
    rule_001_redundant_success_when(&tasks, &index, &mut lints);
    rule_002_skip_for_dependents(&tasks, &mut lints);
    rule_003_004_failure_guarded_tasks(&tasks, &index, &mut lints);
    rule_005_cleanup_via_terminal_task(&tasks, &mut lints);
    rule_006_per_element_timing(&tasks, &mut lints);
    rule_007_manual_sharding(&tasks, &index, &mut lints);

    lints.sort_by(|a, b| {
        let pa = index.get(a.task_id.as_str()).copied().unwrap_or(usize::MAX);
        let pb = index.get(b.task_id.as_str()).copied().unwrap_or(usize::MAX);
        pa.cmp(&pb).then_with(|| a.rule.cmp(b.rule))
    });
    lints
}

// ───────────────────────── shared expression helpers ─────────────────────────

/// The task's `when:` as ONE parsed island (the only spec-valid shape ·
/// `03-dag.md` §when). Multi-island / unparseable strings yield `None`
/// (the analyzer owns those errors · lints stay silent).
fn when_expr(task: &RawTask) -> Option<Expr> {
    let w = task.when.as_ref()?;
    let islands = scan_templates(&w.value).ok()?;
    let mut it = islands.into_iter();
    let island = it.next()?;
    if it.next().is_some() {
        return None;
    }
    Some(island.expr)
}

/// `tasks.<id>.status` (member or `tasks['id'].status` index form) →
/// the referenced task id.
fn task_status_ref(e: &Expr) -> Option<&str> {
    let Expr::Member { base, field } = e else {
        return None;
    };
    if field != "status" {
        return None;
    }
    match base.as_ref() {
        Expr::Member {
            base: root,
            field: id,
        } if matches!(root.as_ref(), Expr::Ident(r) if r == "tasks") => Some(id),
        Expr::Index { base: root, index } if matches!(root.as_ref(), Expr::Ident(r) if r == "tasks") => {
            if let Expr::Lit(Literal::Str(id)) = index.as_ref() {
                Some(id)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// A string literal's value.
fn str_lit(e: &Expr) -> Option<&str> {
    if let Expr::Lit(Literal::Str(s)) = e {
        Some(s)
    } else {
        None
    }
}

/// The WHOLE expression restates the default success gate for ONE task
/// (`tasks.X.status == 'success'` · either operand order · or
/// `tasks.X.status in ['success']`).
fn success_restatement(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Relation {
            op: RelOp::Eq,
            lhs,
            rhs,
        } => {
            if let (Some(id), Some("success")) = (task_status_ref(lhs), str_lit(rhs)) {
                return Some(id);
            }
            if let (Some(id), Some("success")) = (task_status_ref(rhs), str_lit(lhs)) {
                return Some(id);
            }
            None
        }
        Expr::Relation {
            op: RelOp::In,
            lhs,
            rhs,
        } => {
            let id = task_status_ref(lhs)?;
            if let Expr::List(items) = rhs.as_ref()
                && items.len() == 1
                && str_lit(&items[0]) == Some("success")
            {
                return Some(id);
            }
            None
        }
        _ => None,
    }
}

/// Collect task ids whose status is compared against `'failure'`
/// anywhere in the expression (`== 'failure'` · either operand order ·
/// or `in [… 'failure' …]`).
fn collect_failure_checks(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Relation {
            op: RelOp::Eq,
            lhs,
            rhs,
        } => {
            if let (Some(id), Some("failure")) = (task_status_ref(lhs), str_lit(rhs)) {
                out.push(id.to_string());
            }
            if let (Some(id), Some("failure")) = (task_status_ref(rhs), str_lit(lhs)) {
                out.push(id.to_string());
            }
        }
        Expr::Relation {
            op: RelOp::In,
            lhs,
            rhs,
        } => {
            if let Some(id) = task_status_ref(lhs)
                && let Expr::List(items) = rhs.as_ref()
                && items.iter().any(|i| str_lit(i) == Some("failure"))
            {
                out.push(id.to_string());
            }
        }
        Expr::Or(a, b) | Expr::And(a, b) => {
            collect_failure_checks(a, out);
            collect_failure_checks(b, out);
        }
        Expr::Not(a) => collect_failure_checks(a, out),
        _ => {}
    }
}

/// A « permissive » status check — one that deliberately lets non-success
/// states pass (`in [s1, s2, …]` with ≥ 2 statuses · or `!=` against a
/// status string).
fn has_permissive_status_check(e: &Expr) -> bool {
    match e {
        Expr::Relation {
            op: RelOp::In,
            lhs,
            rhs,
        } => {
            task_status_ref(lhs).is_some()
                && matches!(
                    rhs.as_ref(),
                    Expr::List(items)
                        if items.iter().filter(|i| str_lit(i).is_some()).count() >= 2
                )
        }
        Expr::Relation {
            op: RelOp::Ne,
            lhs,
            rhs,
        } => {
            (task_status_ref(lhs).is_some() && str_lit(rhs).is_some())
                || (task_status_ref(rhs).is_some() && str_lit(lhs).is_some())
        }
        Expr::Or(a, b) | Expr::And(a, b) => {
            has_permissive_status_check(a) || has_permissive_status_check(b)
        }
        Expr::Not(a) => has_permissive_status_check(a),
        _ => false,
    }
}

/// Semantic fingerprint of an action (spans stripped) — structural
/// identity for rules 003/007.
fn action_fingerprint(a: &RawAction) -> Value {
    match a {
        RawAction::Exec(e) => json!({
            "verb": "exec",
            "command": e.command.value,
            "cwd": e.cwd.as_ref().map(|s| s.value.clone()),
            "stdin": e.stdin.as_ref().map(|s| s.value.clone()),
            "capture": e.capture.as_ref().map(|c| format!("{:?}", c.value)),
            "env": e
                .env
                .iter()
                .map(|(k, v)| (k.value.clone(), v.value.clone()))
                .collect::<BTreeMap<String, String>>(),
        }),
        RawAction::Invoke(i) => json!({
            "verb": "invoke",
            "tool": i.tool.value,
            "args": i.args.as_ref().map(|v| v.value.clone()),
        }),
        RawAction::Infer(f) => json!({
            "verb": "infer",
            "prompt": f.prompt.value,
            "system": f.system.as_ref().map(|s| s.value.clone()),
            "model": f.model.as_ref().map(|s| s.value.clone()),
        }),
        // Exhaustive on purpose — a NEW verb variant must fail to
        // compile here until its fingerprint is defined (structural
        // enforcement · `#[non_exhaustive]` only binds external crates).
        RawAction::Agent(g) => json!({
            "verb": "agent",
            "prompt": g.prompt.value,
            "system": g.system.as_ref().map(|s| s.value.clone()),
            "model": g.model.as_ref().map(|s| s.value.clone()),
            "tools": g.tools.iter().map(|t| t.value.clone()).collect::<Vec<_>>(),
        }),
    }
}

// ───────────────────────── the 7 rules ─────────────────────────

/// 001 — `when: ${{ tasks.X.status == 'success' }}` with `X ∈
/// depends_on` restates the default edge gate (success-gating IS the
/// default). Exception · when X declares `on_error: skip`, the check is
/// real work (skipped ≠ success) — silent.
fn rule_001_redundant_success_when(
    tasks: &[&RawTask],
    index: &BTreeMap<&str, usize>,
    lints: &mut Vec<Lint>,
) {
    for task in tasks {
        let Some(expr) = when_expr(task) else {
            continue;
        };
        let Some(dep) = success_restatement(&expr) else {
            continue;
        };
        if !task.depends_on.iter().any(|d| d.value == dep) {
            continue;
        }
        // Skip-able dependency → the status check is NOT redundant.
        let dep_skippable = index.get(dep).is_some_and(|&i| {
            matches!(
                tasks[i].on_error.as_ref().map(|o| &o.value),
                Some(OnError::Skip)
            )
        });
        if dep_skippable {
            continue;
        }
        lints.push(Lint::new(
            "one-obvious-way/001",
            task.id.value.clone(),
            task.id.span,
            format!(
                "`when: ${{{{ tasks.{dep}.status == 'success' }}}}` restates the default \
                 edge gate — `depends_on: [{dep}]` alone already success-gates this task"
            ),
            format!("drop the `when:` — `depends_on: [{dep}]` is the one way"),
        ));
    }
}

/// 002 — `on_error: {{ skip: true }}` on a task whose dependents never
/// read its status smuggles « run B even if A failed » into A's
/// contract. The canonical route is an explicit `when:` on the
/// dependent.
fn rule_002_skip_for_dependents(tasks: &[&RawTask], lints: &mut Vec<Lint>) {
    for task in tasks {
        if !matches!(
            task.on_error.as_ref().map(|o| &o.value),
            Some(OnError::Skip)
        ) {
            continue;
        }
        let id = task.id.value.as_str();
        let dependents: Vec<&&RawTask> = tasks
            .iter()
            .filter(|t| t.depends_on.iter().any(|d| d.value == id))
            .collect();
        if dependents.is_empty() {
            continue;
        }
        let unguarded: Vec<&str> = dependents
            .iter()
            .filter(|t| {
                when_expr(t).is_none_or(|expr| {
                    !expr_refs(&expr).iter().any(|r| {
                        matches!(
                            r,
                            NamespaceRef::Tasks { id: rid, field: Some(f) }
                                if rid == id && f == "status"
                        )
                    })
                })
            })
            .map(|t| t.id.value.as_str())
            .collect();
        if unguarded.is_empty() {
            continue;
        }
        lints.push(Lint::new(
            "one-obvious-way/002",
            task.id.value.clone(),
            task.id.span,
            format!(
                "`on_error: skip` changes `{id}`'s contract for its dependents' benefit — \
                 dependent(s) {} never read `tasks.{id}.status`",
                unguarded.join(", ")
            ),
            format!(
                "put an explicit `when:` on the dependent(s) (it replaces the default gate) \
                 — e.g. `when: ${{{{ tasks.{id}.status in ['success', 'failure'] }}}}`"
            ),
        ));
    }
}

/// 003 + 004 — failure-guarded tasks.
///
/// 003 · a `when: tasks.A.status == 'failure'` task whose body is
/// STRUCTURALLY IDENTICAL to A re-implements `retry:`.
///
/// 004 · the same guard around a pure value-producer (conservative
/// subset · `nika:jq` with template-free args · or `exec: echo …`)
/// re-implements `on_error: recover:`. Real failure-work stays silent.
fn rule_003_004_failure_guarded_tasks(
    tasks: &[&RawTask],
    index: &BTreeMap<&str, usize>,
    lints: &mut Vec<Lint>,
) {
    for task in tasks {
        let Some(expr) = when_expr(task) else {
            continue;
        };
        let mut checked = Vec::new();
        collect_failure_checks(&expr, &mut checked);
        let mut fired_003 = false;
        for dep in &checked {
            let Some(&i) = index.get(dep.as_str()) else {
                continue;
            };
            if action_fingerprint(&task.action) == action_fingerprint(&tasks[i].action) {
                lints.push(Lint::new(
                    "one-obvious-way/003",
                    task.id.value.clone(),
                    task.id.span,
                    format!(
                        "failure-guarded duplicate of `{dep}` — a `when:`-guarded copy \
                         re-implements retry"
                    ),
                    format!("put `retry:` on `{dep}` — the ONE retry shape"),
                ));
                fired_003 = true;
                break;
            }
        }
        if fired_003 || checked.is_empty() {
            continue;
        }
        if is_value_producer(&task.action) {
            let dep = &checked[0];
            lints.push(Lint::new(
                "one-obvious-way/004",
                task.id.value.clone(),
                task.id.span,
                format!(
                    "failure-guarded task producing a mere value — the fallback route \
                     belongs in `{dep}` itself"
                ),
                format!(
                    "put `on_error: {{ recover: <value> }}` on `{dep}` — use a task only \
                     when real work runs on failure"
                ),
            ));
        }
    }
}

/// Conservative « mere value » detector for rule 004 · `nika:jq` with
/// template-free args, or an `echo` command with no templates. Anything
/// else counts as real work.
fn is_value_producer(a: &RawAction) -> bool {
    match a {
        RawAction::Invoke(inv) if inv.tool.value == "nika:jq" => {
            inv.args.as_ref().is_none_or(|args| {
                !serde_json::to_string(&args.value)
                    .unwrap_or_default()
                    .contains("${{")
            })
        }
        RawAction::Exec(e) => {
            let c = e.command.value.trim_start();
            c.starts_with("echo ") && !c.contains("${{")
        }
        _ => false,
    }
}

/// 005 — a task depending on EVERY other task with a permissive status
/// `when:` is a hand-rolled cleanup — `on_finally:` is the one way.
fn rule_005_cleanup_via_terminal_task(tasks: &[&RawTask], lints: &mut Vec<Lint>) {
    let n = tasks.len();
    if n < 3 {
        return;
    }
    for task in tasks {
        if task.depends_on.len() != n - 1 {
            continue;
        }
        let deps: BTreeSet<&str> = task.depends_on.iter().map(|d| d.value.as_str()).collect();
        let others: BTreeSet<&str> = tasks
            .iter()
            .map(|t| t.id.value.as_str())
            .filter(|id| *id != task.id.value.as_str())
            .collect();
        if deps != others {
            continue;
        }
        let Some(expr) = when_expr(task) else {
            continue;
        };
        if !has_permissive_status_check(&expr) {
            continue;
        }
        lints.push(Lint::new(
            "one-obvious-way/005",
            task.id.value.clone(),
            task.id.span,
            "terminal task depending on everything with a permissive `when:` — a \
             hand-rolled cleanup"
                .to_string(),
            "use `on_finally:` — cleanup that always runs".to_string(),
        ));
    }
}

/// 006 — wrapping a `for_each` body in `timeout(1)` per element — the
/// task-level `timeout:` covers the whole task (spec `03-dag.md`
/// §timeout).
fn rule_006_per_element_timing(tasks: &[&RawTask], lints: &mut Vec<Lint>) {
    for task in tasks {
        if task.for_each.is_none() {
            continue;
        }
        let RawAction::Exec(e) = &task.action else {
            continue;
        };
        let c = e.command.value.trim_start();
        if !(c.starts_with("timeout ") || c.starts_with("gtimeout ")) {
            continue;
        }
        lints.push(Lint::new(
            "one-obvious-way/006",
            task.id.value.clone(),
            task.id.span,
            "per-element timing trick inside a `for_each` body (`timeout` command wrapper)"
                .to_string(),
            "put `timeout:` on the task — it bounds the whole `for_each` (spec 03 §timeout)"
                .to_string(),
        ));
    }
}

/// 007 — ≥ 3 sequentially-chained tasks that are the SAME operation
/// varying in exactly one slot re-implement fan-out — `for_each` +
/// `max_parallel:` is the one way.
///
/// Conservative shard signature ·
/// - `exec` · same whitespace-token count · exactly ONE differing token
///   position · consistent across the chain.
/// - `invoke` · same tool · args differing in exactly ONE leaf path ·
///   consistent across the chain.
fn rule_007_manual_sharding(
    tasks: &[&RawTask],
    index: &BTreeMap<&str, usize>,
    lints: &mut Vec<Lint>,
) {
    // successor map · only single-dep links count as chain edges.
    let mut successors: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (j, task) in tasks.iter().enumerate() {
        if let [dep] = task.depends_on.as_slice()
            && let Some(&i) = index.get(dep.value.as_str())
        {
            successors.entry(i).or_default().push(j);
        }
    }
    // a chain edge i→j exists when j is i's UNIQUE single-dep successor.
    let next = |i: usize| -> Option<usize> {
        match successors.get(&i).map(Vec::as_slice) {
            Some(&[j]) => Some(j),
            _ => None,
        }
    };
    let has_incoming: BTreeSet<usize> = (0..tasks.len()).filter_map(next).collect();

    for head in 0..tasks.len() {
        if has_incoming.contains(&head) {
            continue; // not a chain head
        }
        let mut chain = vec![head];
        let mut cur = head;
        while let Some(j) = next(cur) {
            chain.push(j);
            cur = j;
        }
        if chain.len() < 3 {
            continue;
        }
        if is_shard_chain(tasks, &chain) {
            let head_task = tasks[chain[0]];
            lints.push(Lint::new(
                "one-obvious-way/007",
                head_task.id.value.clone(),
                head_task.id.span,
                format!(
                    "manual sharding — {} sequential tasks running the same operation \
                     with one varying slot",
                    chain.len()
                ),
                "use ONE task with `for_each:` + `max_parallel:` — the one way to fan out"
                    .to_string(),
            ));
        }
    }
}

/// Does the chain match the conservative shard signature?
fn is_shard_chain(tasks: &[&RawTask], chain: &[usize]) -> bool {
    let actions: Vec<&RawAction> = chain.iter().map(|&i| &tasks[i].action).collect();
    match actions[0] {
        RawAction::Exec(_) => {
            let tokens: Vec<Vec<&str>> = actions
                .iter()
                .filter_map(|a| {
                    if let RawAction::Exec(e) = a {
                        Some(e.command.value.split_whitespace().collect())
                    } else {
                        None
                    }
                })
                .collect();
            if tokens.len() != actions.len() {
                return false; // mixed verbs
            }
            let len = tokens[0].len();
            if tokens.iter().any(|t| t.len() != len) {
                return false;
            }
            // exactly one differing token position · consistent.
            let mut varying: BTreeSet<usize> = BTreeSet::new();
            for pair in tokens.windows(2) {
                for (pos, (a, b)) in pair[0].iter().zip(pair[1].iter()).enumerate() {
                    if a != b {
                        varying.insert(pos);
                    }
                }
            }
            varying.len() == 1
        }
        RawAction::Invoke(first) => {
            let mut prints = Vec::new();
            for a in &actions {
                let RawAction::Invoke(inv) = a else {
                    return false;
                };
                if inv.tool.value != first.tool.value {
                    return false;
                }
                prints.push(inv.args.as_ref().map_or(Value::Null, |v| v.value.clone()));
            }
            // exactly one differing leaf path · consistent.
            let mut varying: BTreeSet<String> = BTreeSet::new();
            for pair in prints.windows(2) {
                varying.extend(differing_leaf_paths(&pair[0], &pair[1]));
            }
            varying.len() == 1
        }
        _ => false,
    }
}

/// Leaf paths (`/a/b/0`) → scalar values.
fn leaf_paths(v: &Value, prefix: &str, out: &mut BTreeMap<String, Value>) {
    match v {
        Value::Object(m) => {
            for (k, vv) in m {
                leaf_paths(vv, &format!("{prefix}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, vv) in a.iter().enumerate() {
                leaf_paths(vv, &format!("{prefix}/{i}"), out);
            }
        }
        _ => {
            out.insert(prefix.to_string(), v.clone());
        }
    }
}

/// The set of leaf paths whose values differ between two JSON values.
fn differing_leaf_paths(a: &Value, b: &Value) -> BTreeSet<String> {
    let mut ma = BTreeMap::new();
    leaf_paths(a, "", &mut ma);
    let mut mb = BTreeMap::new();
    leaf_paths(b, "", &mut mb);
    ma.keys()
        .chain(mb.keys())
        .filter(|k| ma.get(*k) != mb.get(*k))
        .cloned()
        .collect()
}
