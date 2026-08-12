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

use nika_schema::expression::{Expr, NamespaceRef, TemplateIsland, expr_refs, scan_templates};
use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use nika_schema::source::Span;
use nika_schema::types::OnErrorAction;

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
    // one-obvious-way/001 RETIRED in W2 — its discouraged form (a
    // tasks.* status test inside when:) is now ILLEGAL (NIKA-VAR-021);
    // rule ids are stable, the hole is deliberate.
    rule_002_skip_for_dependents(&tasks, &mut lints);
    rule_003_004_failure_guarded_tasks(&tasks, &index, &mut lints);
    rule_005_cleanup_via_terminal_task(&tasks, &mut lints);
    rule_010_non_tightening_after(&tasks, &mut lints);
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

/// `one-obvious-way/010` — an `after: {a: terminal}` beside a VALUE
/// edge to the same producer is a non-tightening restatement: edges
/// compose by intersection, and {success, skipped} ∩ terminal is the
/// value edge alone (spec 03 §one obvious way). Tighten to `success`
/// or drop the entry.
fn rule_010_non_tightening_after(tasks: &[&RawTask], lints: &mut Vec<Lint>) {
    use nika_check::analyzer::edges::{EdgeKind, role_of_field, task_refs_in_value};
    for task in tasks {
        for (target, pred) in &task.after {
            if !matches!(pred.value, nika_schema::types::AfterPredicate::Terminal) {
                continue;
            }
            let has_value_edge = task.with.iter().any(|(_k, v)| {
                let mut refs: Vec<(String, Option<String>)> = Vec::new();
                task_refs_in_value(&v.value, &mut refs);
                refs.iter().any(|(rid, field)| {
                    rid == &target.value
                        && matches!(role_of_field(field.as_deref()), EdgeKind::Value)
                })
            });
            if has_value_edge {
                lints.push(Lint::new(
                    "one-obvious-way/010",
                    task.id.value.clone(),
                    task.id.span,
                    format!(
                        "`after: {{{t}: terminal}}` beside a value edge to `{t}` is a \
                         non-tightening restatement — the composed gate is the value \
                         edge's {{success, skipped}} either way",
                        t = target.value
                    ),
                    format!(
                        "drop the entry or tighten to `after: {{{t}: success}}`",
                        t = target.value
                    ),
                ));
            }
        }
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
        for (name, program) in &task.extract {
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

/// The single `when:` island as a parsed expression (`None` for
/// literals · multi-island · unparseable — the analyzer owns those).
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

/// Semantic fingerprint of an action (spans stripped) — structural
/// identity for rules 003/007.
fn action_fingerprint(a: &RawAction) -> Value {
    match a {
        RawAction::Exec(e) => json!({
            "verb": "exec",
            // Form + parts, never a placeholder: `shell_str().unwrap_or`
            // collapsed EVERY argv command to one constant, so any two
            // failure-guarded exec tasks fingerprinted identical and
            // /003 fired on all of them (the argv sweep's blind helper
            // — the rule_008 class, caught by spec#78's own fixtures).
            "command_form": if e.command.shell_str().is_some() { "shell" } else { "argv" },
            "command": e.command.text_fragments(),
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
            "tool": i.tool().map(|t| t.value.clone()),
            "workflow": i.workflow().map(|w| w.value.clone()),
            "args": i.args.as_ref().map(|v| v.value.clone()),
        }),
        RawAction::Infer(f) => json!({
            "verb": "infer",
            "prompt": f.prompt.value,
            "system": f.system.as_ref().map(|s| s.value.clone()),
            "model": f.model.as_ref().map(|s| s.value.clone()),
        }),
        RawAction::Agent(g) => json!({
            "verb": "agent",
            "prompt": g.prompt.value,
            "system": g.system.as_ref().map(|s| s.value.clone()),
            "model": g.model.as_ref().map(|s| s.value.clone()),
            "tools": g.tools.iter().map(|t| t.value.clone()).collect::<Vec<_>>(),
        }),
        // The 4 verbs are locked forever (D-2026-05-22-N18) — outside the
        // defining crate `#[non_exhaustive]` demands this arm, but by
        // language law it is unreachable; the verb name keeps the
        // fingerprint sound if the law ever changes.
        other => json!({ "verb": other.verb() }),
    }
}

// ───────────────────────── the 7 rules ─────────────────────────

/// 002 — `on_error: {{ skip: true }}` on a task whose dependents never
/// read its status smuggles « run B even if A failed » into A's
/// contract. The canonical route is an explicit `when:` on the
/// dependent.
fn rule_002_skip_for_dependents(tasks: &[&RawTask], lints: &mut Vec<Lint>) {
    use nika_check::analyzer::edges::{EdgeKind, incoming_of};
    for task in tasks {
        if !matches!(
            task.on_error.as_ref().map(|o| &o.value.action),
            Some(OnErrorAction::Skip)
        ) {
            continue;
        }
        let id = task.id.value.as_str();
        // A dependent ACKNOWLEDGES the possible skip when it tightens
        // the gate (`after: {id: success}` — skip cancels it) or its
        // `when:` reads a binding bound to this producer (the null test
        // being the canonical form · spec 03 §gate algebra).
        let mut unguarded: Vec<&str> = Vec::new();
        for t in tasks {
            let value_bindings: Vec<&str> = t
                .with
                .iter()
                .filter(|(_k, v)| {
                    let mut refs = Vec::new();
                    nika_check::analyzer::edges::task_refs_in_value(&v.value, &mut refs);
                    refs.iter().any(|(rid, field)| {
                        rid == id
                            && matches!(
                                nika_check::analyzer::edges::role_of_field(field.as_deref()),
                                EdgeKind::Value
                            )
                    })
                })
                .map(|(k, _v)| k.value.as_str())
                .collect();
            if value_bindings.is_empty() {
                continue;
            }
            let tightened = incoming_of(t).iter().any(|(p, kind)| {
                p == id
                    && matches!(
                        kind,
                        EdgeKind::Control(nika_schema::types::AfterPredicate::Success)
                    )
            });
            let when_reads_binding = when_expr(t).is_some_and(|expr| {
                expr_refs(&expr).iter().any(|r| {
                    matches!(r, NamespaceRef::With(name) if value_bindings.contains(&name.as_str()))
                })
            });
            if !tightened && !when_reads_binding {
                unguarded.push(t.id.value.as_str());
            }
        }
        if unguarded.is_empty() {
            continue;
        }
        lints.push(Lint::new(
            "one-obvious-way/002",
            task.id.value.clone(),
            task.id.span,
            format!(
                "`on_error: skip` changes `{id}`'s contract for its dependents' benefit — \
                 dependent(s) {} read its value without acknowledging the skip (the \
                 binding reads defined-null there)",
                unguarded.join(", ")
            ),
            format!(
                "tighten the dependent's gate (`after: {{{id}: success}}`) or test the \
                 binding in its `when:` (`${{{{ with.<name> != null }}}}`)"
            ),
        ));
    }
}

/// 003 + 004 — failure-path tasks (`after: {a: failure}` · W2).
///
/// 003 · an `after: {a: failure}` task whose body is STRUCTURALLY
/// IDENTICAL to `a` re-implements `retry:`.
///
/// 004 · the same failure path around a pure value-producer
/// (conservative subset · `nika:jq` with template-free args · or
/// `exec: echo …`) re-implements `on_error: recover:`. Real
/// failure-work stays silent.
fn rule_003_004_failure_guarded_tasks(
    tasks: &[&RawTask],
    index: &BTreeMap<&str, usize>,
    lints: &mut Vec<Lint>,
) {
    for task in tasks {
        let checked: Vec<String> = task
            .after
            .iter()
            .filter(|(_t, pred)| matches!(pred.value, nika_schema::types::AfterPredicate::Failure))
            .map(|(t, _)| t.value.clone())
            .collect();
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
                        "failure-path duplicate of `{dep}` — an `after: {{{dep}: failure}}` \
                         copy re-implements retry"
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
                    "failure-path task producing a mere value — the fallback route \
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
        RawAction::Invoke(inv) if inv.tool().map(|t| t.value.as_str()) == Some("nika:jq") => {
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
            // argv `["echo", …]` with template-free elements is the same
            // mere value (spec#78 fixture 004 — the argv sweep's second
            // blind helper in this file).
            None => {
                e.command.argv_program() == Some("echo")
                    && !e.command.text_fragments().iter().any(|f| f.contains("${{"))
            }
        },
        _ => false,
    }
}

/// 005 — a task with `after: {…: terminal}` on EVERY other task is a
/// cleanup smuggled into the graph — `on_finally:` (per task) or ONE
/// terminal report task is the one way.
fn rule_005_cleanup_via_terminal_task(tasks: &[&RawTask], lints: &mut Vec<Lint>) {
    let n = tasks.len();
    if n < 3 {
        return;
    }
    for task in tasks {
        let terminal_targets: BTreeSet<&str> = task
            .after
            .iter()
            .filter(|(_t, pred)| matches!(pred.value, nika_schema::types::AfterPredicate::Terminal))
            .map(|(t, _)| t.value.as_str())
            .collect();
        if terminal_targets.len() != n - 1 {
            continue;
        }
        let others: BTreeSet<&str> = tasks
            .iter()
            .map(|t| t.id.value.as_str())
            .filter(|id| *id != task.id.value.as_str())
            .collect();
        if terminal_targets != others {
            continue;
        }
        lints.push(Lint::new(
            "one-obvious-way/005",
            task.id.value.clone(),
            task.id.span,
            "`after: {…: terminal}` on every other task — a cleanup smuggled into \
             the graph"
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
        // Both forms wear the wrapper: `shell: "timeout 30 …"` and
        // `command: ["gtimeout", "30", …]` (0.103: argv is the default
        // spelling — the D1 migration exposed the shell_str-only blind
        // spot).
        let head_is_timeout = match e.command.shell_str() {
            Some(c) => {
                let c = c.trim_start();
                c.starts_with("timeout ") || c.starts_with("gtimeout ")
            }
            None => matches!(e.command.argv_program(), Some("timeout" | "gtimeout")),
        };
        if !head_is_timeout {
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
    // successor map · only single-producer links count as chain edges.
    let mut successors: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (j, task) in tasks.iter().enumerate() {
        let producers = nika_check::analyzer::edges::producer_ids(task);
        if let [producer] = producers.as_slice()
            && let Some(&i) = index.get(producer.as_str())
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
            // Tokens: argv elements verbatim (0.103's default spelling — the
            // D1 migration exposed the shell_str-only blind spot), else the
            // whitespace-split shell line.
            let tokens: Vec<Vec<&str>> = actions
                .iter()
                .filter_map(|a| {
                    if let RawAction::Exec(e) = a {
                        match &e.command {
                            nika_schema::raw::RawCommand::Argv(parts) => {
                                Some(parts.iter().map(|p| p.value.as_str()).collect())
                            }
                            nika_schema::raw::RawCommand::Shell(c) => {
                                Some(c.value.split_whitespace().collect())
                            }
                            // exec is argv-or-shell by the v0.103 field law —
                            // a third form is spec-gated; advisory pass skips
                            _ => None,
                        }
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
                let same_target = match (&inv.target, &first.target) {
                    (
                        nika_schema::raw::RawInvokeTarget::Tool(a),
                        nika_schema::raw::RawInvokeTarget::Tool(b),
                    )
                    | (
                        nika_schema::raw::RawInvokeTarget::Workflow(a),
                        nika_schema::raw::RawInvokeTarget::Workflow(b),
                    ) => a.value == b.value,
                    _ => false,
                };
                if !same_target {
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
mod tests;
