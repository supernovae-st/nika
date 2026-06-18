// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `one-obvious-way` rule set — spec `03-dag.md` §One obvious way
//! (rules 001-007) + spec `02-verbs.md` §exec Security (rule 008) + spec
//! `04-variables.md` §binding rules (rule 009).
//!
//! Nine preference rules · the spec tables are « normative for linters »
//! · emitted as warnings in table order (`one-obvious-way/001` … `/009`)
//! · never hard errors.
//!
//! Each rule fires only on a PRECISE static signature (low
//! false-positive contract — pinned by `tests/lints_one_obvious_way.rs`).
//! Where the spec row describes intent the validator cannot decide
//! statically (« a mere value » · « manual sharding » · « an interpolated
//! value that needs no shell »), the rule implements a documented
//! conservative subset.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::expression::{
    Expr, Literal, NamespaceRef, RelOp, TemplateIsland, expr_refs, scan_templates,
};
use crate::raw::{RawAction, RawTask, RawWorkflow};
use crate::source::Span;
use crate::types::OnErrorAction;

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
    rule_008_interpolated_string_command(&tasks, &mut lints);
    rule_009_stream_binding(&tasks, &mut lints);

    lints.sort_by(|a, b| {
        let pa = index.get(a.task_id.as_str()).copied().unwrap_or(usize::MAX);
        let pb = index.get(b.task_id.as_str()).copied().unwrap_or(usize::MAX);
        pa.cmp(&pb).then_with(|| a.rule.cmp(b.rule))
    });
    lints
}

/// `one-obvious-way/008` — an interpolated value in a STRING `exec.command`
/// that needs no shell (spec `02-verbs.md` §exec Security). The array form
/// passes the value as ONE argv token (injection-safe); the string form
/// shell-parses it. Fires ONLY when the LITERAL command parts carry no shell
/// metacharacter — a genuine pipeline/redirect/glob legitimately keeps the
/// string form (`/bin/sh -c` is exactly what it is for), so the rule must
/// not flag it (the low-false-positive contract).
fn rule_008_interpolated_string_command(tasks: &[&RawTask], lints: &mut Vec<Lint>) {
    for task in tasks {
        let RawAction::Exec(e) = &task.action else {
            continue;
        };
        let Some(command) = e.command.shell_str() else {
            continue; // already the array form — nothing to steer
        };
        let Ok(islands) = scan_templates(command) else {
            continue; // a malformed template is the analyzer's error, not ours
        };
        if islands.is_empty() || literal_parts_use_shell(command, &islands) {
            // no interpolation, OR a genuine shell line (pipe/redirect/glob).
            continue;
        }
        lints.push(Lint::new(
            "one-obvious-way/008",
            task.id.value.clone(),
            task.id.span,
            "an interpolated value lands in a string `command` that needs no \
             shell — it is shell-parsed (the command-injection surface)"
                .to_string(),
            "use the array form `command: [prog, \"${{ … }}\", …]` — each element \
             is one literal argv token, so the value cannot break out (spec §exec)"
                .to_string(),
        ));
    }
}

/// `one-obvious-way/009` — an `output:` binding whose jq program ends in a
/// bare iterator `[]` with no collecting `[ … ]` wrapper (spec
/// `04-variables.md` §binding rules · « the reference linter additionally WARNS
/// at check time on the statically-visible smell »). A binding resolves to
/// exactly ONE value; a trailing `[]` emits a STREAM whose count is
/// data-dependent → runtime `NIKA-VAR-002`. One lint per task (the first smelly
/// binding · `break`), anchored at the offending jq island.
fn rule_009_stream_binding(tasks: &[&RawTask], lints: &mut Vec<Lint>) {
    for task in tasks {
        for (name, program) in &task.output {
            if ends_in_bare_iterator(&program.value) {
                lints.push(Lint::new(
                    "one-obvious-way/009",
                    task.id.value.clone(),
                    program.span,
                    format!(
                        "the `{}` binding's jq ends in a bare iterator `[]` — it \
                         emits a stream, but a binding resolves to exactly ONE value \
                         (the count is data-dependent · fails at runtime · NIKA-VAR-002)",
                        name.value
                    ),
                    "collect the stream with `[ … ]` (`[.users[]]` → array) or take \
                     one with an index (`.users[0]`) / `first(…)` (spec 04 §binding rules)"
                        .to_string(),
                ));
                break; // one lint per task · the first smelly binding
            }
        }
    }
}

/// Whether a jq program ends in a bare ITERATOR `[]` (a stream emitter) and not
/// an empty-array LITERAL (`.a // []`). The iterator applies to a value, so the
/// char before the final `[` is a path/value char (`s` in `.users[]`, `.` in
/// `.[]`, `)` in `(…)[]`) — a literal `[]` is preceded by an operator/space.
fn ends_in_bare_iterator(program: &str) -> bool {
    let Some(stripped) = program.trim_end().strip_suffix("[]") else {
        return false;
    };
    matches!(
        stripped.chars().last(),
        Some(c) if c.is_alphanumeric() || c == '_' || c == ')' || c == ']' || c == '.'
    )
}

/// Whether the LITERAL parts of `command` (the `${{ }}` islands removed)
/// carry a shell metacharacter — i.e. the author genuinely needs `/bin/sh
/// -c` (a pipe, redirect, sub-shell, glob, shell variable, …). When they do,
/// the string form is correct and [`rule_008_interpolated_string_command`]
/// must not fire.
fn literal_parts_use_shell(command: &str, islands: &[TemplateIsland]) -> bool {
    const SHELL_META: &[char] = &[
        '|', '&', ';', '<', '>', '(', ')', '$', '`', '*', '?', '{', '}',
    ];
    let mut cursor = 0;
    for island in islands {
        if command[cursor..island.start].contains(SHELL_META) {
            return true;
        }
        cursor = island.end;
    }
    command[cursor..].contains(SHELL_META)
}

// ───────────────────────── shared expression helpers ─────────────────────────

/// The task's `when:` as ONE parsed island (the only spec-valid shape ·
/// `03-dag.md` §when). Multi-island / unparseable strings yield `None`
/// (the analyzer owns those errors · lints stay silent).
fn when_expr(task: &RawTask) -> Option<Expr> {
    let w = task.when.as_ref()?;
    let src = w.value.as_expr()?; // boolean literals carry no expression
    let islands = scan_templates(src).ok()?;
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
            "command": e.command.shell_str().unwrap_or("<argv>"),
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
                tasks[i].on_error.as_ref().map(|o| &o.value.action),
                Some(OnErrorAction::Skip)
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
            task.on_error.as_ref().map(|o| &o.value.action),
            Some(OnErrorAction::Skip)
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
        RawAction::Exec(e) => match e.command.shell_str() {
            Some(c) => {
                let c = c.trim_start();
                c.starts_with("echo ") && !c.contains("${{")
            }
            None => false, // argv form has no shell echo
        },
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
        let Some(c) = e.command.shell_str() else {
            continue; // argv form: no shell timeout wrapper
        };
        let c = c.trim_start();
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
                        e.command
                            .shell_str()
                            .map(|c| c.split_whitespace().collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::{parse_expression, scan_templates};
    use crate::raw::{RawExecAction, RawInferAction, RawInvokeAction};
    use crate::source::{Span, Spanned};
    use crate::{FileId, ParseMode, parse};
    use serde_json::{Value, json};

    // ───────────────────────── rule-level helpers ─────────────────────────

    /// Parse a workflow fixture + run all 9 preference rules.
    fn lints_of(yaml: &str) -> Vec<Lint> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        one_obvious_way(&wf)
    }

    /// Only the lints for one rule id (keeps assertions immune to other rules).
    fn lints_for(yaml: &str, rule: &str) -> Vec<Lint> {
        lints_of(yaml)
            .into_iter()
            .filter(|l| l.rule == rule)
            .collect()
    }

    // ──────────────── expression-helper grounding ────────────────
    //
    // The helpers borrow `&str` out of an `Expr`, so the `Expr` is bound to a
    // local first (`task_status_ref` / `str_lit` return references into it).

    /// Parse a bare CEL expression (the inside of a `${{ … }}` island).
    fn expr(src: &str) -> Expr {
        parse_expression(src).expect("expression parses")
    }

    /// The first `${{ … }}` island's expression, parsed from a raw string —
    /// what `literal_parts_use_shell` consumes alongside the raw `command`.
    fn islands_of(s: &str) -> Vec<TemplateIsland> {
        scan_templates(s).expect("templates scan")
    }

    // ─────────────────────────────────────────────────────────────────────
    // rule_008_interpolated_string_command   (108:5 body→() · 118:31 ||→&&)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn rule_008_fires_on_interpolated_string_command() {
        // 108:5 — replacing the whole fn body with `()` emits no lint.
        let yaml = "\
nika: v1
workflow: interp
tasks:
  - id: produce
    exec: { command: \"./gen.sh\" }
  - id: consume
    depends_on: [produce]
    exec: { command: \"process ${{ tasks.produce.output }}\" }
";
        let eight = lints_for(yaml, "one-obvious-way/008");
        assert_eq!(eight.len(), 1, "exactly one /008 must fire");
        assert_eq!(eight[0].task_id, "consume");
    }

    #[test]
    fn rule_008_silent_on_a_plain_literal_command() {
        // 118:31 — `islands.is_empty() || literal_parts_use_shell(..)` decides
        // « skip ». For `"cargo build"`: islands EMPTY (true) · shell-meta NONE
        // (false). The correct `||` ⇒ true ⇒ skip ⇒ no lint. The `&&` mutant ⇒
        // `true && false` ⇒ false ⇒ FALL THROUGH ⇒ a spurious /008 on a command
        // that has no interpolation at all. Asserting zero kills the swap.
        let yaml = "\
nika: v1
workflow: plain
tasks:
  - id: build
    exec: { command: \"cargo build\" }
";
        assert!(
            lints_for(yaml, "one-obvious-way/008").is_empty(),
            "a non-interpolated command must never be flagged"
        );
    }

    #[test]
    fn rule_008_silent_on_a_genuine_pipeline() {
        // The OTHER half of the 118:31 `||`: islands NON-empty but the literal
        // parts carry a `|` ⇒ `literal_parts_use_shell` true ⇒ skip. (Also pins
        // the `literal_parts_use_shell` true-branch reachability.)
        let yaml = "\
nika: v1
workflow: pipe
tasks:
  - id: produce
    exec: { command: \"./gen.sh\" }
  - id: consume
    depends_on: [produce]
    exec: { command: \"cat ${{ tasks.produce.output }} | wc -l\" }
";
        assert!(
            lints_for(yaml, "one-obvious-way/008").is_empty(),
            "a genuine shell pipeline keeps the string form"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // rule_009_stream_binding                              (144:5 body→())
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn rule_009_fires_on_a_bare_iterator_binding() {
        // 144:5 — replacing the fn body with `()` emits no lint.
        let yaml = "\
nika: v1
workflow: stream
tasks:
  - id: fetch
    invoke: { tool: \"nika:read\", args: { path: \"u.json\" } }
    output:
      emails: \".users[]\"
";
        let nine = lints_for(yaml, "one-obvious-way/009");
        assert_eq!(nine.len(), 1, "exactly one /009 must fire");
        assert_eq!(nine[0].task_id, "fetch");
        assert!(nine[0].message.contains("emails"), "{}", nine[0].message);
    }

    // ─────────────────────────────────────────────────────────────────────
    // ends_in_bare_iterator                       (172:5 →true · 172:5 →false)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn ends_in_bare_iterator_true_on_a_stream() {
        // 172:5 `-> false` mutant: a genuine bare iterator must return TRUE.
        assert!(ends_in_bare_iterator(".users[]"));
        assert!(ends_in_bare_iterator(".[]"));
        assert!(ends_in_bare_iterator("(.a)[]"));
        assert!(ends_in_bare_iterator(".users[] ")); // trailing ws trimmed
    }

    #[test]
    fn ends_in_bare_iterator_false_on_a_literal_or_scalar() {
        // 172:5 `-> true` mutant: a non-iterator must return FALSE.
        assert!(!ends_in_bare_iterator(".a // []")); // empty-array literal default
        assert!(!ends_in_bare_iterator(".users")); // no trailing `[]`
        assert!(!ends_in_bare_iterator(".users[0]")); // indexed take
    }

    // ─────────────────────────────────────────────────────────────────────
    // literal_parts_use_shell                                (187:5 →true)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn literal_parts_use_shell_false_without_metacharacters() {
        // 187:5 `-> true` mutant: an interpolated command whose literal parts
        // carry NO shell metacharacter must return FALSE (so /008 fires).
        let cmd = "process ${{ tasks.t.output }} now";
        assert!(!literal_parts_use_shell(cmd, &islands_of(cmd)));
    }

    #[test]
    fn literal_parts_use_shell_true_with_a_pipe() {
        // The complementary TRUE case (a real pipeline) — also distinguishes
        // 187:5 from a constant-false would-be mutant.
        let cmd = "cat ${{ tasks.t.output }} | wc -l";
        assert!(literal_parts_use_shell(cmd, &islands_of(cmd)));
    }

    // ─────────────────────────────────────────────────────────────────────
    // when_expr                                              (206:5 →None)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn when_expr_some_drives_rule_001() {
        // 206:5 `-> None` mutant: rule_001 only fires when `when_expr` returns
        // Some — a forced None silences /001 entirely.
        let yaml = "\
nika: v1
workflow: redundant
tasks:
  - id: a
    exec: { command: \"./a.sh\" }
  - id: b
    depends_on: [a]
    when: \"${{ tasks.a.status == 'success' }}\"
    exec: { command: \"./b.sh\" }
";
        let one = lints_for(yaml, "one-obvious-way/001");
        assert_eq!(one.len(), 1, "the redundant success `when:` must fire /001");
        assert_eq!(one[0].task_id, "b");
    }

    // ─────────────────────────────────────────────────────────────────────
    // task_status_ref   (220:5 →None/Some("") · 223:14 !=/== · 230/231 guard)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn task_status_ref_member_form_returns_the_id() {
        // 220:5 (→None / →Some("")) AND 230 guard (→false): `tasks.<id>.status`
        // must resolve to exactly the id, not None and not "".
        let e = expr("tasks.build.status");
        assert_eq!(task_status_ref(&e), Some("build"));
    }

    #[test]
    fn task_status_ref_index_form_returns_the_id() {
        // 231 guard (→false): `tasks['id'].status` must resolve to the id.
        let e = expr("tasks['deploy'].status");
        assert_eq!(task_status_ref(&e), Some("deploy"));
    }

    #[test]
    fn task_status_ref_rejects_a_non_status_field() {
        // 223:14 (`!=` → `==`): the guard rejects non-`status` fields. With the
        // mutant the guard rejects `status` instead, so `tasks.x.status` would
        // return None — already covered above. Here the converse: a NON-status
        // field must be None (with `==` it would wrongly fall through).
        let e = expr("tasks.build.output");
        assert_eq!(task_status_ref(&e), None);
    }

    #[test]
    fn task_status_ref_rejects_a_non_tasks_root_member() {
        // 230 guard (→true): the Member arm must REJECT a non-`tasks` root.
        // A forced-true guard would wrongly accept `vars.x.status`.
        let e = expr("vars.x.status");
        assert_eq!(task_status_ref(&e), None);
    }

    #[test]
    fn task_status_ref_rejects_a_non_tasks_root_index() {
        // 231 guard (→true): the Index arm must REJECT a non-`tasks` root.
        let e = expr("vars['x'].status");
        assert_eq!(task_status_ref(&e), None);
    }

    // ─────────────────────────────────────────────────────────────────────
    // str_lit                                  (244:5 →None/Some("")/Some(xyzzy))
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn str_lit_returns_the_exact_literal() {
        // 244:5 — the three constant mutants (None / "" / "xyzzy") all die
        // against the exact value.
        let e = expr("'hello'");
        assert_eq!(str_lit(&e), Some("hello"));
        // and a non-string literal is None.
        let n = expr("42");
        assert_eq!(str_lit(&n), None);
    }

    // ─────────────────────────────────────────────────────────────────────
    // success_restatement                  (256:9 del Eq arm · 269:9 del In arm)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn success_restatement_matches_the_eq_form() {
        // 256:9 — deleting the Eq arm drops to `_ => None`.
        let e = expr("tasks.build.status == 'success'");
        assert_eq!(success_restatement(&e), Some("build"));
        // operand order is symmetric.
        let flipped = expr("'success' == tasks.build.status");
        assert_eq!(success_restatement(&flipped), Some("build"));
    }

    #[test]
    fn success_restatement_matches_the_in_form() {
        // 269:9 — deleting the In arm drops to `_ => None`.
        let e = expr("tasks.build.status in ['success']");
        assert_eq!(success_restatement(&e), Some("build"));
    }

    #[test]
    fn success_restatement_silent_on_a_failure_check() {
        // a `== 'failure'` is NOT a success restatement.
        let e = expr("tasks.build.status == 'failure'");
        assert_eq!(success_restatement(&e), None);
    }

    // ─────────────────────────────────────────────────────────────────────
    // collect_failure_checks       (291:5 →() · 316 del Or/And · 320 del Not)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn collect_failure_checks_collects_the_eq_form() {
        // 291:5 — replacing the body with `()` leaves `out` empty.
        let e = expr("tasks.build.status == 'failure'");
        let mut out = Vec::new();
        collect_failure_checks(&e, &mut out);
        assert_eq!(out, vec!["build".to_string()]);
        // operand order is symmetric.
        let flipped = expr("'failure' == tasks.build.status");
        let mut out2 = Vec::new();
        collect_failure_checks(&flipped, &mut out2);
        assert_eq!(out2, vec!["build".to_string()]);
    }

    #[test]
    fn collect_failure_checks_collects_the_in_form() {
        let e = expr("tasks.build.status in ['failure', 'cancelled']");
        let mut out = Vec::new();
        collect_failure_checks(&e, &mut out);
        assert_eq!(out, vec!["build".to_string()]);
    }

    #[test]
    fn collect_failure_checks_descends_or_and_and() {
        // 316:9 — deleting the `Or | And` arm stops the recursion descending.
        let or = expr("tasks.a.status == 'failure' || tasks.b.status == 'failure'");
        let mut out = Vec::new();
        collect_failure_checks(&or, &mut out);
        assert_eq!(out, vec!["a".to_string(), "b".to_string()]);

        let and = expr("tasks.a.status == 'failure' && tasks.b.status == 'failure'");
        let mut out2 = Vec::new();
        collect_failure_checks(&and, &mut out2);
        assert_eq!(out2, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn collect_failure_checks_descends_not() {
        // 320:9 — deleting the `Not` arm stops the recursion descending.
        let e = expr("!(tasks.a.status == 'failure')");
        let mut out = Vec::new();
        collect_failure_checks(&e, &mut out);
        assert_eq!(out, vec!["a".to_string()]);
    }

    // ─────────────────────────────────────────────────────────────────────
    // has_permissive_status_check
    //   (330 del In · 342 del Ne · 353 del Not · 336:17 && · 347:45 && · 348:17 ||)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn permissive_in_with_two_statuses_is_permissive() {
        // 330:9 — deleting the In arm drops to `_ => false`.
        let e = expr("tasks.x.status in ['success', 'failure']");
        assert!(has_permissive_status_check(&e));
    }

    #[test]
    fn permissive_in_with_one_status_is_not_permissive() {
        // 336:17 (`&&` → `||`): `task_status_ref(lhs).is_some() && matches!(≥2)`.
        // A single-status list ⇒ is_some()=true, matches(≥2)=false ⇒ `&&`=false.
        // The `||` mutant ⇒ true. A one-status `in` is NOT permissive.
        let e = expr("tasks.x.status in ['success']");
        assert!(!has_permissive_status_check(&e));
    }

    #[test]
    fn permissive_ne_against_a_status_is_permissive() {
        // 342:9 — deleting the Ne arm drops to `_ => false`.
        // 348:17 (`||` → `&&`): only the FIRST clause is true here
        //   (task_status on lhs · str literal on rhs) ⇒ `||`=true · `&&`=false.
        let e = expr("tasks.x.status != 'success'");
        assert!(has_permissive_status_check(&e));
        // operand order is symmetric (status on the rhs).
        let flipped = expr("'success' != tasks.x.status");
        assert!(has_permissive_status_check(&flipped));
    }

    #[test]
    fn permissive_ne_between_two_task_statuses_is_not_permissive() {
        // 347:45 (`&&` → `||`): clause 1 is
        //   `task_status_ref(lhs).is_some() && str_lit(rhs).is_some()`.
        // For `tasks.x.status != tasks.y.status`: lhs is a task-status (true) but
        // rhs is NOT a string literal (false) ⇒ clause1 `&&`=false. Clause 2 is
        // also false (rhs task-status, lhs not a literal). So the whole thing is
        // false. The `||` mutant makes clause1 true ⇒ wrongly permissive.
        let e = expr("tasks.x.status != tasks.y.status");
        assert!(!has_permissive_status_check(&e));
    }

    #[test]
    fn permissive_descends_not() {
        // 353:9 — deleting the Not arm drops to `_ => false`. The inner Ne is
        // permissive, so the negation must descend and report true.
        let e = expr("!(tasks.x.status != 'success')");
        assert!(has_permissive_status_check(&e));
    }

    #[test]
    fn permissive_strict_eq_is_not_permissive() {
        // a strict `== 'success'` gate is the default, NOT permissive.
        let e = expr("tasks.x.status == 'success'");
        assert!(!has_permissive_status_check(&e));
    }

    // ─────────────────────────────────────────────────────────────────────
    // rule_002_skip_for_dependents
    //   (447:5 body→() · 448:12 del ! · 466:21 del ! · 457:61 ==→!=)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn rule_002_fires_for_an_unguarded_dependent() {
        // 447:5 (body→()), 448:12 (del `!` ⇒ processes only NON-skip tasks),
        // 457:61 (`==`→`!=` ⇒ the dependents set becomes the tasks that do NOT
        // depend on the skip task ⇒ empty here ⇒ no lint). All three silence the
        // expected single /002.
        let yaml = "\
nika: v1
workflow: skipdep
tasks:
  - id: a
    on_error: { skip: true }
    exec: { command: \"./a.sh\" }
  - id: b
    depends_on: [a]
    exec: { command: \"./b.sh\" }
";
        let two = lints_for(yaml, "one-obvious-way/002");
        assert_eq!(
            two.len(),
            1,
            "an unguarded dependent of a skip task fires /002"
        );
        assert_eq!(two[0].task_id, "a");
        assert!(two[0].message.contains('b'), "{}", two[0].message);
    }

    #[test]
    fn rule_002_silent_when_the_dependent_reads_status() {
        // 466:21 (del `!`): `!expr_refs(..).any(reads tasks.a.status)` decides
        // « unguarded ». The dependent HERE reads `tasks.a.status`, so it IS
        // guarded ⇒ no /002. Deleting the `!` inverts this ⇒ the guarded
        // dependent is wrongly treated as unguarded ⇒ a spurious /002.
        let yaml = "\
nika: v1
workflow: guarded
tasks:
  - id: a
    on_error: { skip: true }
    exec: { command: \"./a.sh\" }
  - id: b
    depends_on: [a]
    when: \"${{ tasks.a.status in ['success', 'failure'] }}\"
    exec: { command: \"./b.sh\" }
";
        assert!(
            lints_for(yaml, "one-obvious-way/002").is_empty(),
            "a dependent that reads the status is guarded — no /002"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // rule_003_004_failure_guarded_tasks                       (521:49 ==→!=)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn rule_003_fires_on_a_structural_duplicate() {
        // 521:49 (`==`→`!=`): /003 fires when the failure-guarded task's action
        // fingerprint EQUALS the guarded dep's. The `!=` mutant inverts this:
        // identical actions ⇒ `!=` false ⇒ no /003 (and the exec body is not a
        // value-producer ⇒ no /004 either) ⇒ zero lints.
        let yaml = "\
nika: v1
workflow: dup
tasks:
  - id: build
    exec: { command: \"./build.sh\" }
  - id: rebuild
    depends_on: [build]
    when: \"${{ tasks.build.status == 'failure' }}\"
    exec: { command: \"./build.sh\" }
";
        let three = lints_for(yaml, "one-obvious-way/003");
        assert_eq!(
            three.len(),
            1,
            "a failure-guarded structural copy fires /003"
        );
        assert_eq!(three[0].task_id, "rebuild");
    }

    // ───────────────────────── direct-construction helpers ─────────────────────────
    //
    // `is_value_producer` / `is_shard_chain` / `leaf_paths` / `differing_leaf_paths`
    // / `action_fingerprint` take `&RawAction` / `&[&RawTask]` / `&Value`, not a
    // parsed string — so we build the AST nodes directly (same crate · the
    // `#[non_exhaustive]` ratchet only binds external crates).

    /// A zero-span `Spanned<String>`.
    fn sp(s: &str) -> Spanned<String> {
        Spanned::new(s.to_owned(), Span::default())
    }

    /// An `exec:` action with a shell-string command.
    fn exec_cmd(cmd: &str) -> RawAction {
        RawAction::Exec(RawExecAction::new(sp(cmd)))
    }

    /// An `invoke:` action — `tool` + optional template-args JSON.
    fn invoke_tool(tool: &str, args: Option<Value>) -> RawAction {
        let mut a = RawInvokeAction::new(sp(tool));
        a.args = args.map(|v| Spanned::new(v, Span::default()));
        RawAction::Invoke(a)
    }

    /// A bare `infer:` action (never a value-producer, never a shard).
    fn infer_prompt(prompt: &str) -> RawAction {
        RawAction::Infer(Box::new(RawInferAction::new(sp(prompt))))
    }

    /// A task carrying an action — id is irrelevant to the helpers under test.
    fn task_with(id: &str, action: RawAction) -> RawTask {
        RawTask::new(sp(id), action)
    }

    // ─────────────────────────────────────────────────────────────────────
    // collect_failure_checks · the `in`-list scan         (311:52 `==`→`!=`)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn collect_failure_checks_in_single_element_failure_list() {
        // 311:52 (`str_lit(i) == Some("failure")` → `!=`): a ONE-element
        // `['failure']` list. With the correct `==`, `any(.. == failure)` is
        // true ⇒ push. With `!=`, `any(.. != failure)` over `['failure']` is
        // FALSE ⇒ nothing pushed. (A multi-element list would mask the swap —
        // a second status makes `!=` true again — so the list must be singleton.)
        let e = expr("tasks.build.status in ['failure']");
        let mut out = Vec::new();
        collect_failure_checks(&e, &mut out);
        assert_eq!(out, vec!["build".to_string()]);
    }

    // ─────────────────────────────────────────────────────────────────────
    // has_permissive_status_check · the Or/And arm   (350:9 del · 351:44 `||`→`&&`)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn permissive_descends_or_with_one_permissive_side() {
        // 350:9 (delete the `Or | And` arm ⇒ `_ => false`) AND 351:44
        // (`||` → `&&`). The left disjunct is permissive (`!= 'success'`),
        // the right is a strict `== 'success'` (NOT permissive). Correct:
        // `has(left) || has(right)` = `true || false` = true. Deleting the arm
        // ⇒ false. The `&&` mutant ⇒ `true && false` = false. One assertion
        // (true) kills both.
        let e = expr("tasks.x.status != 'success' || tasks.y.status == 'success'");
        assert!(has_permissive_status_check(&e));
    }

    #[test]
    fn permissive_and_with_one_permissive_side() {
        // The And half of the shared `Or | And` arm — same one-true-one-false
        // shape, so it independently exercises the descent on `And`.
        let e = expr("tasks.x.status != 'success' && tasks.y.status == 'success'");
        assert!(has_permissive_status_check(&e));
    }

    // ─────────────────────────────────────────────────────────────────────
    // action_fingerprint                            (361:5 → Default::default())
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn action_fingerprint_is_a_real_structural_print() {
        // 361:5 (body → `Default::default()` ⇒ `Value::Null`): the print must
        // NOT be null, must carry the verb, and must DISTINGUISH two different
        // actions (the whole point — rule 003/007 compare prints for equality).
        let fp_exec = action_fingerprint(&exec_cmd("./build.sh"));
        assert_ne!(fp_exec, Value::Null);
        assert_eq!(fp_exec.get("verb").and_then(Value::as_str), Some("exec"));

        let fp_invoke = action_fingerprint(&invoke_tool("nika:read", None));
        assert_ne!(fp_exec, fp_invoke, "different verbs ⇒ different prints");

        // identical exec bodies ⇒ identical prints (the equality rule 003 uses).
        assert_eq!(fp_exec, action_fingerprint(&exec_cmd("./build.sh")));
        // different commands ⇒ different prints.
        assert_ne!(fp_exec, action_fingerprint(&exec_cmd("./other.sh")));
    }

    // ─────────────────────────────────────────────────────────────────────
    // is_value_producer
    //   (562:5 →false/→true · 563:35 guard · 563:50 `==`→`!=` · 565:17 del !
    //    · 570:9 del Exec arm · 573:40 `&&`→`||` · 573:43 del !)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn value_producer_jq_without_templates_is_one() {
        // 562:5 (→false), 563:35 (guard →false), 563:50 (`==`→`!=`): a `nika:jq`
        // with template-free args IS a value-producer. All three mutants make
        // this return false.
        assert!(is_value_producer(&invoke_tool(
            "nika:jq",
            Some(json!({ "filter": ".name" }))
        )));
        // and with NO args at all (is_none_or short-circuits true).
        assert!(is_value_producer(&invoke_tool("nika:jq", None)));
    }

    #[test]
    fn value_producer_jq_with_templates_is_not_one() {
        // 565:17 (delete `!`): the closure is `!serialized.contains("${{")`.
        // jq args carrying `${{ … }}` ⇒ contains ⇒ `!` ⇒ false (NOT a value-
        // producer). Deleting the `!` inverts it ⇒ wrongly a value-producer.
        assert!(!is_value_producer(&invoke_tool(
            "nika:jq",
            Some(json!({ "filter": "${{ tasks.a.output }}" }))
        )));
    }

    #[test]
    fn value_producer_non_jq_invoke_is_not_one() {
        // 563:35 (guard →true): forcing the `tool == "nika:jq"` guard true makes
        // ANY invoke take the jq arm. A `nika:fetch` with no args would then
        // return true (is_none_or). The correct guard rejects it ⇒ false.
        assert!(!is_value_producer(&invoke_tool("nika:fetch", None)));
    }

    #[test]
    fn value_producer_echo_exec_is_one() {
        // 570:9 (delete the `Exec` arm ⇒ `_ => false`): a template-free `echo`
        // command IS a value-producer. Deleting the arm returns false.
        assert!(is_value_producer(&exec_cmd("echo hello")));
        // leading whitespace is trimmed before the `echo ` check.
        assert!(is_value_producer(&exec_cmd("   echo hi")));
    }

    #[test]
    fn value_producer_non_echo_exec_is_not_one() {
        // 573:40 (`&&` → `||`): the body is
        //   `c.starts_with("echo ") && !c.contains("${{")`.
        // For `"ls -la"`: starts_with("echo ")=false · contains("${{")=false ⇒
        // `!`=true. Correct `&&` ⇒ false && true = false. The `||` mutant ⇒
        // false || true = true ⇒ wrongly a value-producer.
        assert!(!is_value_producer(&exec_cmd("ls -la")));
    }

    #[test]
    fn value_producer_echo_with_template_is_not_one() {
        // 573:43 (delete the second `!`): for `echo ${{ … }}`: starts_with
        // ("echo ")=true · contains("${{")=true ⇒ `!`=false ⇒ true && false =
        // false (NOT a value-producer · the value is interpolated work).
        // Deleting the `!` ⇒ true && true = true ⇒ wrongly a value-producer.
        assert!(!is_value_producer(&exec_cmd("echo ${{ tasks.a.output }}")));
    }

    #[test]
    fn value_producer_argv_and_infer_are_not_value_producers() {
        // 562:5 (→true): a non-jq invoke is already covered; here the catch-all
        // `_ => false` paths — an `infer` action and an argv-form exec (no shell
        // string) must both be false. A forced-true body wrongly flags them.
        assert!(!is_value_producer(&infer_prompt("summarize this")));
        let argv = RawAction::Exec(RawExecAction::with_command(
            crate::raw::action::RawCommand::Argv(vec![sp("echo"), sp("hi")]),
        ));
        assert!(!is_value_producer(&argv));
    }

    // ─────────────────────────────────────────────────────────────────────
    // is_shard_chain
    //   exec branch (713:5 →false/→true · 716:9 del Exec · 729:29 · 733:46
    //                · 740:26 · 745:27)
    //   invoke branch (747:9 del Invoke · 753:35 · 763:27)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn is_shard_chain_exec_three_token_single_varying_slot() {
        // 713:5 (→false), 716:9 (delete the Exec arm ⇒ `_=>false`), 729:29
        // (`tokens.len() != actions.len()` mixed-verb guard ⇒ `==` returns
        // false on a homogeneous chain), 733:46 (`t.len() != len` differing-
        // length guard ⇒ `==` returns false on equal-length tokens), 745:27
        // (`varying.len() == 1` ⇒ `!=` returns false): a genuine exec shard
        // chain (3 tokens · ONLY the last varies) returns true. Three tokens
        // (not two) is required for 740:26 below.
        let tasks = [
            task_with("s1", exec_cmd("./p a x")),
            task_with("s2", exec_cmd("./p a y")),
            task_with("s3", exec_cmd("./p a z")),
        ];
        let refs: Vec<&RawTask> = tasks.iter().collect();
        assert!(is_shard_chain(&refs, &[0, 1, 2]));
    }

    #[test]
    fn is_shard_chain_exec_inserts_varying_not_equal_positions() {
        // 740:26 (`if a != b` ⇒ `==`): the loop records VARYING token positions.
        // With three tokens where pos 0+1 are constant (`./p`, `a`) and pos 2
        // varies, the correct set is {2} (len 1 ⇒ true). The `==` mutant records
        // the EQUAL positions {0, 1} (len 2 ⇒ false). Two equal positions are
        // needed so the mutant's set size ≠ 1 (a 2-token chain would give the
        // equal-set size 1 too and mask the swap).
        let tasks = [
            task_with("s1", exec_cmd("./p a x")),
            task_with("s2", exec_cmd("./p a y")),
            task_with("s3", exec_cmd("./p a z")),
        ];
        let refs: Vec<&RawTask> = tasks.iter().collect();
        assert!(is_shard_chain(&refs, &[0, 1, 2]));

        // negative control · TWO token positions vary ⇒ not a single-slot shard.
        let multi = [
            task_with("s1", exec_cmd("./p a x")),
            task_with("s2", exec_cmd("./p b y")),
            task_with("s3", exec_cmd("./p c z")),
        ];
        let multi_refs: Vec<&RawTask> = multi.iter().collect();
        assert!(!is_shard_chain(&multi_refs, &[0, 1, 2]));
    }

    #[test]
    fn is_shard_chain_rejects_a_genuine_pipeline() {
        // 713:5 (→true): different programs / differing token counts is NOT a
        // shard chain. A forced-true body wrongly accepts it.
        let tasks = [
            task_with("fetch", exec_cmd("./fetch.sh")),
            task_with("parse", exec_cmd("./parse.sh --strict input.json")),
            task_with("report", exec_cmd("./report.sh")),
        ];
        let refs: Vec<&RawTask> = tasks.iter().collect();
        assert!(!is_shard_chain(&refs, &[0, 1, 2]));
    }

    #[test]
    fn is_shard_chain_invoke_same_tool_single_varying_leaf() {
        // 747:9 (delete the Invoke arm ⇒ `_=>false`), 753:35 (`inv.tool !=
        // first.tool` mixed-tool guard ⇒ `==` returns false on a same-tool
        // chain), 763:27 (`varying.len() == 1` ⇒ `!=` returns false): same tool,
        // args differing in exactly ONE leaf path (`/url`) ⇒ true.
        let tasks = [
            task_with(
                "p1",
                invoke_tool("nika:fetch", Some(json!({ "url": "https://x/1" }))),
            ),
            task_with(
                "p2",
                invoke_tool("nika:fetch", Some(json!({ "url": "https://x/2" }))),
            ),
            task_with(
                "p3",
                invoke_tool("nika:fetch", Some(json!({ "url": "https://x/3" }))),
            ),
        ];
        let refs: Vec<&RawTask> = tasks.iter().collect();
        assert!(is_shard_chain(&refs, &[0, 1, 2]));
    }

    #[test]
    fn is_shard_chain_invoke_rejects_mixed_tools() {
        // 753:35 (`!=` → `==`) complement: DIFFERENT tools across the chain ⇒
        // the guard must return false. With `==` the same-tool guard inverts and
        // a mixed-tool chain would slip through — pin the correct false here.
        let tasks = [
            task_with(
                "p1",
                invoke_tool("nika:fetch", Some(json!({ "url": "https://x/1" }))),
            ),
            task_with(
                "p2",
                invoke_tool("nika:read", Some(json!({ "url": "https://x/2" }))),
            ),
            task_with(
                "p3",
                invoke_tool("nika:fetch", Some(json!({ "url": "https://x/3" }))),
            ),
        ];
        let refs: Vec<&RawTask> = tasks.iter().collect();
        assert!(!is_shard_chain(&refs, &[0, 1, 2]));
    }

    // ─────────────────────────────────────────────────────────────────────
    // leaf_paths                       (771:5 →() · 772:9 del Object · 777:9 del Array)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn leaf_paths_recurses_into_objects() {
        // 771:5 (body → `()` ⇒ nothing inserted) AND 772:9 (delete the `Object`
        // arm ⇒ the whole object is inserted at the prefix instead of recursing).
        // Correct: an object yields one key per field (`/a`, `/b`), NOT a single
        // `""` → {whole object}.
        let mut out = BTreeMap::new();
        leaf_paths(&json!({ "a": 1, "b": 2 }), "", &mut out);
        let keys: Vec<&String> = out.keys().collect();
        assert_eq!(keys, vec!["/a", "/b"]);
        assert_eq!(out.get("/a"), Some(&json!(1)));
        assert_eq!(out.get("/b"), Some(&json!(2)));
    }

    #[test]
    fn leaf_paths_recurses_into_arrays() {
        // 777:9 (delete the `Array` arm ⇒ the whole array inserted at the
        // prefix). Correct: an array yields one key per index (`/0`, `/1`).
        let mut out = BTreeMap::new();
        leaf_paths(&json!([10, 20]), "", &mut out);
        let keys: Vec<&String> = out.keys().collect();
        assert_eq!(keys, vec!["/0", "/1"]);
        assert_eq!(out.get("/0"), Some(&json!(10)));
    }

    // ─────────────────────────────────────────────────────────────────────
    // differing_leaf_paths
    //   (790:5 →empty/`[""]`/`["xyzzy"]` · 796:32 `!=`→`==`)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn differing_leaf_paths_reports_the_changed_path_only() {
        // 790:5 (→`BTreeSet::new()` / `[""]` / `["xyzzy"]`): two objects sharing
        // `/x` and differing at `/u` ⇒ the differing set is EXACTLY {`/u`}. The
        // empty-set mutant gives {} · the `[""]` mutant gives {""} · the
        // `["xyzzy"]` mutant gives {"xyzzy"} — all ≠ {"/u"}.
        // 796:32 (`ma.get != mb.get` → `==`): the filter keeps paths whose values
        // DIFFER. With `==` it would keep the EQUAL path `/x` instead ⇒ {"/x"}.
        let a = json!({ "x": 1, "u": "p1" });
        let b = json!({ "x": 1, "u": "p2" });
        let diff = differing_leaf_paths(&a, &b);
        let expected: BTreeSet<String> = ["/u".to_string()].into_iter().collect();
        assert_eq!(diff, expected);
    }

    #[test]
    fn differing_leaf_paths_empty_when_identical() {
        // Complement of 796:32 — identical values share every path, so the
        // correct `!=` filter keeps NONE. (The `==` mutant would keep them all.)
        let a = json!({ "x": 1, "u": "p1" });
        assert!(differing_leaf_paths(&a, &a).is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────
    // rule_003_004_failure_guarded_tasks   (536:22 `||`→`&&` · the empty-check guard)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn rule_004_silent_when_guard_has_no_failure_check() {
        // 536:22 (`if fired_003 || checked.is_empty()` → `&&`): a SUCCESS `when:`
        // collects no failure check ⇒ `checked` is empty · `fired_003` is false.
        // Correct `||` ⇒ `false || true` = true ⇒ `continue` (no /004). The `&&`
        // mutant ⇒ `false && true` = false ⇒ falls through to `checked[0]` —
        // an index into an EMPTY vec ⇒ panic (a panic is a killed mutant). Either
        // way the assertion (no /003, no /004) holds only for the correct code.
        let yaml = "\
nika: v1
workflow: succguard
tasks:
  - id: a
    exec: { command: \"echo hi\" }
  - id: b
    depends_on: [a]
    when: \"${{ tasks.a.status == 'success' }}\"
    invoke: { tool: \"nika:jq\", args: { filter: \".x\" } }
";
        assert!(
            lints_for(yaml, "one-obvious-way/004").is_empty(),
            "a success `when:` collects no failure check ⇒ no /004"
        );
        assert!(
            lints_for(yaml, "one-obvious-way/003").is_empty(),
            "no failure-guarded duplicate either"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // rule_005_cleanup_via_terminal_task
    //   (583:5 →() · 585:10 `<` · 589:34/39 · 596:30 · 598:17 · 604:12 del !)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn rule_005_fires_on_terminal_cleanup_depending_on_everything() {
        // 583:5 (→()), 585:10 (`<`→`<=`/`==` ⇒ n=3 wrongly returns early),
        // 589:34 (`!=`→`==` ⇒ the depends-on-all task is skipped), 589:39
        // (`n-1`→`n+1`/`n/1` ⇒ the count never matches), 596:30 (`!=`→`==` ⇒
        // `others` becomes {self} ≠ deps), 598:17 (`!=`→`==` ⇒ equal deps wrongly
        // `continue`), 604:12 (delete `!` ⇒ a permissive `when:` wrongly skips):
        // a 3-task workflow whose terminal task depends on BOTH others with a
        // permissive `when:` fires exactly one /005.
        let yaml = "\
nika: v1
workflow: f005
tasks:
  - id: a
    exec: { command: \"./a.sh\" }
  - id: b
    exec: { command: \"./b.sh\" }
  - id: cleanup
    depends_on: [a, b]
    when: \"${{ tasks.a.status in ['success', 'failure'] }}\"
    exec: { command: \"./cleanup.sh\" }
";
        let five = lints_for(yaml, "one-obvious-way/005");
        assert_eq!(five.len(), 1, "the terminal cleanup fires /005");
        assert_eq!(five[0].task_id, "cleanup");
    }

    #[test]
    fn rule_005_silent_on_a_two_task_workflow() {
        // 585:10 (`n < 3` → `>`): with the `>` mutant, `2 > 3` is false ⇒ the
        // guard does NOT return ⇒ a 2-task « depends-on-the-other » permissive
        // task would wrongly fire /005. The correct `<` returns early (a real
        // cleanup needs ≥ 3 tasks). Asserting zero kills the `>` swap.
        let yaml = "\
nika: v1
workflow: f005two
tasks:
  - id: a
    exec: { command: \"./a.sh\" }
  - id: b
    depends_on: [a]
    when: \"${{ tasks.a.status != 'success' }}\"
    exec: { command: \"./b.sh\" }
";
        assert!(
            lints_for(yaml, "one-obvious-way/005").is_empty(),
            "fewer than 3 tasks is not a cleanup-of-everything"
        );
    }

    #[test]
    fn rule_005_silent_when_not_depending_on_everything() {
        // Negative control · the permissive `when:` task depends on only ONE of
        // two other tasks ⇒ `deps != others` ⇒ no /005. Pins 596:30 / 598:17
        // from the other direction (deps must EQUAL the full others set).
        let yaml = "\
nika: v1
workflow: f005partial
tasks:
  - id: a
    exec: { command: \"./a.sh\" }
  - id: b
    exec: { command: \"./b.sh\" }
  - id: c
    depends_on: [a]
    when: \"${{ tasks.a.status in ['success', 'failure'] }}\"
    exec: { command: \"./c.sh\" }
";
        assert!(
            lints_for(yaml, "one-obvious-way/005").is_empty(),
            "depending on a strict subset is not a cleanup-of-everything"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // rule_006_per_element_timing      (623:5 →() · 634:12 del ! · 634:40 `||`→`&&`)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn rule_006_fires_on_a_timeout_wrapper_in_for_each() {
        // 623:5 (→()), 634:12 (delete `!` ⇒ a `timeout `-prefixed command wrongly
        // `continue`s), 634:40 (`||`→`&&` ⇒ `(starts "timeout " && starts
        // "gtimeout ")` is false for a plain `timeout ` line ⇒ `!false`=true ⇒
        // wrongly `continue`): a `for_each` body whose command wraps `timeout`
        // fires exactly one /006.
        let yaml = "\
nika: v1
workflow: f006
tasks:
  - id: shards
    for_each: [1, 2, 3]
    exec: { command: \"timeout 30 ./process.sh\" }
";
        let six = lints_for(yaml, "one-obvious-way/006");
        assert_eq!(six.len(), 1, "the per-element timeout wrapper fires /006");
        assert_eq!(six[0].task_id, "shards");
    }

    #[test]
    fn rule_006_fires_on_a_gtimeout_wrapper() {
        // The `gtimeout ` half of the 634:40 `||`: with the `&&` mutant the
        // gtimeout line also fails to fire (`starts "timeout "` is false).
        let yaml = "\
nika: v1
workflow: f006g
tasks:
  - id: shards
    for_each: [1, 2, 3]
    exec: { command: \"gtimeout 30 ./process.sh\" }
";
        let six = lints_for(yaml, "one-obvious-way/006");
        assert_eq!(six.len(), 1, "the gtimeout wrapper fires /006");
        assert_eq!(six[0].task_id, "shards");
    }

    #[test]
    fn rule_006_silent_on_a_plain_for_each_command() {
        // Negative control · a `for_each` body with no timeout wrapper ⇒ no /006
        // (pins the `starts_with("timeout ")` precision · low false-positive).
        let yaml = "\
nika: v1
workflow: f006ok
tasks:
  - id: shards
    for_each: [1, 2, 3]
    timeout: 30s
    exec: { command: \"./process.sh\" }
";
        assert!(
            lints_for(yaml, "one-obvious-way/006").is_empty(),
            "a plain for_each command is not a per-element timing trick"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // rule_007_manual_sharding
    //   (664:5 →() · 675:13 del `Some(&[j])` arm · 691:24 `<`)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn rule_007_fires_on_three_sequential_exec_shards() {
        // 664:5 (→()), 675:13 (delete the `Some(&[j])` arm of `next` ⇒ no chain
        // edges ⇒ every chain is length 1 ⇒ never fires), 691:24 (`chain.len()
        // < 3` → `<=`/`==` ⇒ a length-3 chain wrongly `continue`s): three
        // single-dep exec shards differing in one token fire exactly one /007 on
        // the head.
        let yaml = "\
nika: v1
workflow: f007
tasks:
  - id: shard1
    exec: { command: \"./process.sh part1\" }
  - id: shard2
    depends_on: [shard1]
    exec: { command: \"./process.sh part2\" }
  - id: shard3
    depends_on: [shard2]
    exec: { command: \"./process.sh part3\" }
";
        let seven = lints_for(yaml, "one-obvious-way/007");
        assert_eq!(seven.len(), 1, "the 3-shard chain fires /007 on the head");
        assert_eq!(seven[0].task_id, "shard1");
    }

    #[test]
    fn rule_007_fires_on_three_sequential_invoke_shards() {
        // Drives the Invoke branch of `is_shard_chain` through the rule — same
        // tool, args differing in one leaf path — covering 747/753/763 end-to-end
        // (in addition to the direct helper tests above).
        let yaml = "\
nika: v1
workflow: f007invoke
tasks:
  - id: page1
    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com/page/1\" } }
  - id: page2
    depends_on: [page1]
    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com/page/2\" } }
  - id: page3
    depends_on: [page2]
    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com/page/3\" } }
";
        let seven = lints_for(yaml, "one-obvious-way/007");
        assert_eq!(seven.len(), 1, "the 3-invoke-shard chain fires /007");
        assert_eq!(seven[0].task_id, "page1");
    }

    #[test]
    fn rule_007_silent_on_a_two_task_chain() {
        // 691:24 (`chain.len() < 3` → `>`): with the `>` mutant `2 > 3` is false
        // ⇒ a length-2 shard chain wrongly proceeds to `is_shard_chain` (which is
        // true for two single-slot shards) ⇒ a spurious /007. The correct `<`
        // skips chains under 3. Asserting zero kills the `>` swap.
        let yaml = "\
nika: v1
workflow: f007two
tasks:
  - id: shard1
    exec: { command: \"./process.sh part1\" }
  - id: shard2
    depends_on: [shard1]
    exec: { command: \"./process.sh part2\" }
";
        assert!(
            lints_for(yaml, "one-obvious-way/007").is_empty(),
            "a 2-task chain is below the ≥3 shard threshold"
        );
    }

    #[test]
    fn rule_007_silent_on_a_genuine_pipeline() {
        // Negative control · 3 chained tasks that are NOT the same operation
        // (different programs / token counts) ⇒ `is_shard_chain` false ⇒ no /007.
        let yaml = "\
nika: v1
workflow: f007pipe
tasks:
  - id: fetch
    exec: { command: \"./fetch.sh\" }
  - id: parse
    depends_on: [fetch]
    exec: { command: \"./parse.sh --strict input.json\" }
  - id: report
    depends_on: [parse]
    exec: { command: \"./report.sh\" }
";
        assert!(
            lints_for(yaml, "one-obvious-way/007").is_empty(),
            "a genuine pipeline is not manual sharding"
        );
    }
}
